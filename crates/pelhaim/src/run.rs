use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use pelhaim_adapters::observe::{ObserveClaude, ObserveCodex, ObserveOptions};
use pelhaim_adapters::{AdapterCommand, SessionAdapter};
use pelhaim_board::scene::LABEL_ROW_H;
use pelhaim_board::{
    DisplayList, PaletteKind, R, SceneInput, ViewState, board_layout, build_scene, fit,
};
use pelhaim_core::{Command, Engine, Event, Input, LineKey, Snapshot, Tool};
use pelhaim_gpu::AppOptions;
use pelhaim_store::Store;
use pelhaim_store::decision_log::{LogProject, open_decisions};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::backend::Backend;
use crate::config::{Config, Loaded, Motion, Palette};
use crate::engine_task::{EngineHandle, input_channel};
use crate::env::Env;
use crate::keep_awake::{Holder, Os, detect};
use crate::store_worker::StoreWorker;

pub const CATCH_UP_TIMEOUT: Duration = Duration::from_secs(30);
pub const DUMP_VIEWPORT: (f32, f32) = (1600.0, 900.0);
const DECISIONS_SHOWN: usize = 50;
const ADAPTER_EVENTS: usize = 1024;

/// F44 and the Task 11 ruling: adapters and the engine task run on worker threads, so the
/// UI thread stays free for winit and for `RemoteApi`'s blocking calls.
pub fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

pub fn db_path(env: &dyn Env) -> PathBuf {
    Config::data_dir(env).join("pelhaim.db")
}

pub fn log_projects(cfg: &Config) -> Vec<LogProject> {
    cfg.project
        .iter()
        .map(|p| LogProject::new(&p.slug, &p.name, p.log_section.as_deref()))
        .collect()
}

pub fn local_offset() -> chrono::FixedOffset {
    *chrono::Local::now().offset()
}

pub fn this_os() -> Os {
    if cfg!(target_os = "macos") {
        Os::MacOs
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else {
        Os::Other
    }
}

/// Task 17 ruling: on macOS only the system `caffeinate` counts, so a different one
/// earlier on $PATH can never be picked up.
pub fn keep_awake_search(os: Os, env: &dyn Env) -> OsString {
    match os {
        Os::MacOs => OsString::from("/usr/bin"),
        Os::Linux | Os::Other => OsString::from(env.var("PATH").unwrap_or_default()),
    }
}

pub fn holder(cfg: &Config, env: &dyn Env) -> Holder {
    let os = this_os();
    Holder::new(
        detect(&keep_awake_search(os, env), os),
        std::process::id(),
        cfg.keep_awake.keep_display_awake,
    )
}

pub fn app_options(cfg: &Config) -> AppOptions {
    AppOptions {
        palette: match cfg.board.palette {
            Palette::Matte => PaletteKind::Matte,
            Palette::Film => PaletteKind::Film,
            Palette::HighContrast => PaletteKind::HighContrast,
        },
        reduced_motion: cfg.board.motion == Motion::Reduced,
        ..AppOptions::default()
    }
}

pub fn init_tracing(env: &dyn Env) -> anyhow::Result<()> {
    let dir = Config::data_dir(env).join("logs");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join("pelhaim.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .map_err(|e| anyhow::anyhow!("starting the log: {e}"))
}

/// Spawns the store worker and the engine task, then asks for every decision a crash or
/// restart left unexported.
pub fn start_engine(
    rt: &tokio::runtime::Handle,
    cfg: &Config,
    env: &dyn Env,
    holder: Holder,
) -> anyhow::Result<EngineHandle> {
    let db = db_path(env);
    let store = Store::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let restored = store.load().context("loading the store")?;
    tracing::info!(
        assignments = restored.assignments.len(),
        decisions = restored.decisions.len(),
        "store loaded"
    );
    let engine = Engine::new(cfg.engine_config(local_offset()), restored);
    let (tx, rx) = input_channel();
    let worker = StoreWorker::spawn(
        store,
        cfg.decision_log_path(env),
        log_projects(cfg),
        tx.clone(),
    );
    let handle = EngineHandle::spawn(rt, engine, worker, holder, (tx, rx));
    handle.send(Input::Command(Command::ExportPending));
    Ok(handle)
}

pub fn adapters(cfg: &Config, env: &dyn Env, once: bool) -> Vec<Box<dyn SessionAdapter>> {
    let opts = |home| ObserveOptions {
        once,
        ..ObserveOptions::new(home, cfg.observe.active_window.0)
    };
    vec![
        Box::new(ObserveClaude::new(opts(cfg.claude_home(env)))),
        Box::new(ObserveCodex::new(opts(cfg.codex_home(env)))),
    ]
}

/// Each adapter forwards through the awaited `send_event` (F44), so a large catch-up waits
/// for queue space instead of dropping events. The returned senders keep the adapters alive.
pub fn spawn_observers(
    rt: &tokio::runtime::Handle,
    adapters: Vec<Box<dyn SessionAdapter>>,
    handle: &EngineHandle,
) -> Vec<mpsc::Sender<AdapterCommand>> {
    adapters
        .into_iter()
        .map(|a| {
            let (etx, mut erx) = mpsc::channel(ADAPTER_EVENTS);
            let (ctx, crx) = mpsc::channel(4);
            let kind = a.kind();
            rt.spawn(async move {
                if let Err(e) = a.run(etx, crx).await {
                    tracing::error!(adapter = ?kind, error = %e, "adapter stopped");
                }
            });
            let h = handle.clone();
            rt.spawn(async move {
                while let Some(ev) = erx.recv().await {
                    h.send_event(Input::Event(ev)).await;
                }
            });
            ctx
        })
        .collect()
}

pub fn observe(loaded: &Loaded, env: &dyn Env) -> anyhow::Result<()> {
    let cfg = &loaded.config;
    let rt = runtime()?;
    let handle = start_engine(rt.handle(), cfg, env, holder(cfg, env))?;
    let commands = spawn_observers(rt.handle(), adapters(cfg, env, false), &handle);
    tracing::info!(projects = cfg.project.len(), "observe started");
    let backend = Backend::new(handle, rt.handle().clone());
    let result = pelhaim_gpu::run_app(Box::new(backend), app_options(cfg));
    drop(commands);
    rt.shutdown_timeout(Duration::from_secs(5));
    result
}

/// Runs one-shot adapters and returns every event up to both `CatchUpDone`s.
pub async fn catch_up(
    adapters: Vec<Box<dyn SessionAdapter>>,
    timeout: Duration,
) -> anyhow::Result<Vec<Event>> {
    let (tx, mut rx) = mpsc::channel(ADAPTER_EVENTS);
    let n = adapters.len();
    let mut keep_alive = Vec::with_capacity(n);
    for a in adapters {
        let (ctx, crx) = mpsc::channel(1);
        keep_alive.push(ctx);
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = a.run(tx, crx).await {
                tracing::error!(error = %e, "adapter catch-up failed");
            }
        });
    }
    drop(tx);
    let deadline = tokio::time::Instant::now() + timeout;
    let mut events = Vec::new();
    let mut done: Vec<Tool> = Vec::new();
    while done.len() < n {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) => {
                if let Event::CatchUpDone { tool } = &ev
                    && !done.contains(tool)
                {
                    done.push(*tool);
                }
                events.push(ev);
            }
            Ok(None) => bail!("an adapter stopped before catching up"),
            Err(_) => bail!("catch-up did not finish within {} s", timeout.as_secs()),
        }
    }
    Ok(events)
}

#[derive(Serialize)]
pub struct SceneDump {
    pub viewport: (f32, f32),
    #[serde(flatten)]
    pub list: DisplayList,
    /// Screen-space label rows, so a reader can find a line's pixels without the layout code.
    pub lines: Vec<LineRow>,
}

#[derive(Serialize)]
pub struct LineRow {
    pub key: LineKey,
    pub label_row: R,
}

/// F30: the default view is fitted to the board before drawing.
pub fn scene_dump(snap: &Snapshot, opts: &AppOptions) -> SceneDump {
    let mut view = ViewState::default();
    let layout = board_layout(snap, &view.expanded);
    let v = fit(layout.content, DUMP_VIEWPORT);
    (view.k, view.ox, view.oy) = (v.k, v.ox, v.oy);
    let scene = build_scene(&SceneInput {
        snap,
        view: &view,
        viewport: DUMP_VIEWPORT,
        now_s: 0.0,
        palette: opts.palette,
        reduced_motion: opts.reduced_motion,
    });
    let lines = layout
        .lines
        .iter()
        .map(|l| LineRow {
            key: l.key.clone(),
            label_row: v.rect(R {
                x: layout.content.x,
                y: l.top,
                w: layout.content.w,
                h: LABEL_ROW_H,
            }),
        })
        .collect();
    SceneDump {
        viewport: DUMP_VIEWPORT,
        list: scene.list,
        lines,
    }
}

/// F51: events fold into a local engine synchronously, with no engine task, so the
/// snapshot taken after both `CatchUpDone`s holds every event.
pub fn dump_scene(loaded: &Loaded, env: &dyn Env, out: &Path) -> anyhow::Result<()> {
    let cfg = &loaded.config;
    let db = db_path(env);
    let restored = Store::open(&db)
        .and_then(|s| s.load())
        .with_context(|| format!("loading {}", db.display()))?;
    let mut engine = Engine::new(cfg.engine_config(local_offset()), restored);
    let events = runtime()?.block_on(catch_up(adapters(cfg, env, true), CATCH_UP_TIMEOUT))?;
    tracing::info!(events = events.len(), "headless catch-up done");
    for ev in events {
        engine.apply(Input::Event(ev));
    }
    let dump = scene_dump(&engine.snapshot(), &app_options(cfg));
    let json = serde_json::to_vec(&dump)?;
    std::fs::write(out, json).with_context(|| format!("writing {}", out.display()))
}

/// Read-only: never creates the store or the log.
pub fn decisions(loaded: &Loaded, env: &dyn Env, open: bool) -> anyhow::Result<String> {
    let cfg = &loaded.config;
    let mut out = String::new();
    if open {
        let path = cfg.decision_log_path(env);
        let doc = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(format!("decision log {} not found\n", path.display()));
            }
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        for (slug, items) in open_decisions(&doc, &log_projects(cfg)) {
            if items.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{slug} ({})", items.len());
            for item in items {
                let _ = writeln!(out, "  {item}");
            }
        }
        if out.is_empty() {
            out.push_str("no open decisions\n");
        }
        return Ok(out);
    }
    let db = db_path(env);
    if !db.exists() {
        return Ok("no decisions logged yet\n".into());
    }
    let rows = Store::open(&db)
        .and_then(|s| s.decisions(DECISIONS_SHOWN))
        .with_context(|| format!("reading {}", db.display()))?;
    if rows.is_empty() {
        return Ok("no decisions logged yet\n".into());
    }
    for d in rows {
        let state = if d.written_to_md {
            "exported"
        } else {
            "not exported"
        };
        let _ = writeln!(out, "{}  {}  {}  ({state})", d.date, d.project, d.title);
        let _ = writeln!(out, "    Why: {}", d.why);
        let _ = writeln!(out, "    Who: {}", d.who.as_log_str());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MapEnv;
    use pelhaim_core::{AlertId, Decision, DecisionId, Origin, OwnerEvent, Who};
    use std::time::Instant;

    #[test]
    fn keep_awake_on_macos_searches_usr_bin_only() {
        let env = MapEnv::new("/h").with("PATH", "/evil/bin:/usr/bin");
        assert_eq!(
            keep_awake_search(Os::MacOs, &env),
            OsString::from("/usr/bin")
        );
        assert_eq!(
            keep_awake_search(Os::Linux, &env),
            OsString::from("/evil/bin:/usr/bin")
        );
    }

    #[test]
    fn palette_and_motion_map_from_config() {
        let mut c = Config::default();
        assert_eq!(app_options(&c).palette, PaletteKind::Matte);
        assert!(!app_options(&c).reduced_motion);
        c.board.palette = Palette::HighContrast;
        c.board.motion = Motion::Reduced;
        assert_eq!(app_options(&c).palette, PaletteKind::HighContrast);
        assert!(app_options(&c).reduced_motion);
    }

    fn decision(id: i64, origin: Origin) -> Decision {
        Decision {
            id: DecisionId(id),
            project: "ethica".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
            title: format!("Decision {id}"),
            why: "test".into(),
            who: Who::Owner,
            origin,
            source_alert: Some(AlertId(format!("a{id}"))),
            written_to_md: false,
        }
    }

    /// A decision logged before a crash, but never exported, reaches the markdown on the next start,
    /// and its origin (who answered) survives the restart.
    #[test]
    fn startup_exports_pending_decisions_and_keeps_origin() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join("Developer")).unwrap();
        let log = home.join("Developer/DECISION-LOG.md");
        std::fs::write(
            &log,
            "# Decision log\n\n## ethica\n\n### Decided\n\n---\n\n## Standing rules\n",
        )
        .unwrap();
        let data = dir.path().join("data");
        let env = MapEnv::new(&home)
            .with("PELHAIM_DATA_DIR", data.to_str().unwrap())
            .with("CLAUDE_CONFIG_DIR", dir.path().join("c").to_str().unwrap())
            .with("CODEX_HOME", dir.path().join("x").to_str().unwrap());
        let mut cfg = Config::default();
        cfg.project.push(crate::config::ProjectEntry {
            slug: "ethica".into(),
            name: "ethica".into(),
            path: None,
            repo: None,
            order: 1,
            log_section: None,
        });
        let phone = Origin::Remote {
            device_id: "phone".into(),
        };
        let mut store = Store::open(&db_path(&env)).unwrap();
        store
            .append(
                &OwnerEvent::DecisionLogged(decision(7, phone.clone())),
                chrono::Utc::now(),
            )
            .unwrap();
        drop(store);

        let rt = runtime().unwrap();
        let handle = start_engine(
            rt.handle(),
            &cfg,
            &env,
            Holder::new(None, std::process::id(), false),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !std::fs::read_to_string(&log)
            .unwrap()
            .contains("Decision 7")
        {
            assert!(
                Instant::now() < deadline,
                "pending decision was not exported"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        while handle.snapshot().decisions_unexported != 0 {
            assert!(Instant::now() < deadline, "export never acknowledged");
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(handle);
        rt.shutdown_timeout(Duration::from_secs(5));

        let restored = Store::open(&db_path(&env)).unwrap().load().unwrap();
        let d = restored
            .decisions
            .iter()
            .find(|d| d.id == DecisionId(7))
            .unwrap();
        assert_eq!(d.origin, phone);
        assert!(d.written_to_md);
        let doc = std::fs::read_to_string(&log).unwrap();
        assert_eq!(doc.matches("Decision 7").count(), 1);
    }
}
