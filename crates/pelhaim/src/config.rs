use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::FixedOffset;
use pelhaim_core::engine::{EngineConfig, ProjectCfg};
use pelhaim_core::{DISPATCH_SLUG, is_valid_slug};
use serde::{Deserialize, Deserializer};

use crate::env::{Env, expand_tilde};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Observe,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DurationStr(pub Duration);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OptDuration(pub Option<Duration>);

impl<'de> Deserialize<'de> for DurationStr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match parse_duration(&s).map_err(serde::de::Error::custom)? {
            Some(v) => Ok(Self(v)),
            None => Err(serde::de::Error::custom("\"off\" is not allowed here")),
        }
    }
}

impl<'de> Deserialize<'de> for OptDuration {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_duration(&s)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub mode: Vec<Mode>,
    pub observe: ObserveCfg,
    pub project: Vec<ProjectEntry>,
    pub dispatch: DispatchCfg,
    pub keep_awake: KeepAwakeCfg,
    pub remote: RemoteCfg,
    pub decision_log: DecisionLogCfg,
    pub board: BoardCfg,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: vec![Mode::Observe],
            observe: ObserveCfg::default(),
            project: Vec::new(),
            dispatch: DispatchCfg::default(),
            keep_awake: KeepAwakeCfg::default(),
            remote: RemoteCfg::default(),
            decision_log: DecisionLogCfg::default(),
            board: BoardCfg::default(),
        }
    }
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectEntry {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub log_section: Option<String>,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct ObserveCfg {
    pub claude_home: Option<String>,
    pub codex_home: Option<String>,
    pub active_window: DurationStr,
    pub stall_after: OptDuration,
    pub permission_after: OptDuration,
    pub result_alerts: bool,
}

impl Default for ObserveCfg {
    fn default() -> Self {
        Self {
            claude_home: None,
            codex_home: None,
            active_window: DurationStr(Duration::from_secs(48 * 3600)),
            stall_after: OptDuration(Some(Duration::from_secs(10 * 60))),
            permission_after: OptDuration(Some(Duration::from_secs(20))),
            result_alerts: false,
        }
    }
}

#[derive(Clone, PartialEq, Default, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct DispatchCfg {
    pub sessions: Vec<String>,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct KeepAwakeCfg {
    pub enabled: bool,
    pub include_observed: bool,
    pub keep_display_awake: bool,
}

impl Default for KeepAwakeCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            include_observed: true,
            keep_display_awake: false,
        }
    }
}

#[derive(Clone, PartialEq, Default, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct RemoteCfg {
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LogWrite {
    OnClick,
    Off,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct DecisionLogCfg {
    pub path: String,
    pub write: LogWrite,
}

impl Default for DecisionLogCfg {
    fn default() -> Self {
        Self {
            path: "~/Developer/DECISION-LOG.md".to_string(),
            write: LogWrite::OnClick,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    Matte,
    Film,
    HighContrast,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Full,
    Reduced,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct BoardCfg {
    pub palette: Palette,
    pub motion: Motion,
    pub float_alert_lines: bool,
}

impl Default for BoardCfg {
    fn default() -> Self {
        Self {
            palette: Palette::Matte,
            motion: Motion::Full,
            float_alert_lines: true,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Loaded {
    pub config: Config,
    pub source: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound {
        path: PathBuf,
    },
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Invalid {
        key: String,
        message: String,
    },
    Duration {
        value: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "config file {} does not exist", path.display()),
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Parse { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Invalid { key, message } => write!(f, "{key}: {message}"),
            Self::Duration { value } => write!(
                f,
                "invalid duration {value:?}: expected <int>s, <int>m, <int>h or \"off\""
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn parse_duration(s: &str) -> Result<Option<Duration>, ConfigError> {
    if s == "off" {
        return Ok(None);
    }
    let bad = || ConfigError::Duration {
        value: s.to_string(),
    };
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let n: u64 = num.parse().map_err(|_| bad())?;
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return Err(bad()),
    };
    n.checked_mul(scale)
        .map(|secs| Some(Duration::from_secs(secs)))
        .ok_or_else(bad)
}

const LATER_SECTIONS: [&str; 5] = ["launch", "direct", "approval", "project_overrides", "git"];

pub fn load(explicit: Option<&Path>, env: &dyn Env) -> Result<Loaded, ConfigError> {
    let (path, must_exist) = match (explicit, env.var("PELHAIM_CONFIG")) {
        (Some(p), _) => (p.to_path_buf(), true),
        (None, Some(p)) => (expand_tilde(&p, env), true),
        (None, None) => (env.home().join(".config/pelhaim/config.toml"), false),
    };
    let (config, source) = match std::fs::read_to_string(&path) {
        Ok(text) => (parse(&text, &path)?, Some(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if must_exist {
                return Err(ConfigError::NotFound { path });
            }
            (Config::default(), None)
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    config.validate(env)?;
    Ok(Loaded { config, source })
}

/// Milestone checks run on the raw table first: once serde sees `[launch]` or
/// `write = "auto"`, `deny_unknown_fields` only produces a generic error.
fn parse(text: &str, path: &Path) -> Result<Config, ConfigError> {
    let parse_err = |e: toml::de::Error| ConfigError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    };
    let raw: toml::Table = toml::from_str(text).map_err(parse_err)?;
    for section in LATER_SECTIONS {
        if raw.contains_key(section) {
            return Err(invalid(
                section,
                format!("section [{section}] is not supported until M2"),
            ));
        }
    }
    if let Some(modes) = raw.get("mode").and_then(toml::Value::as_array) {
        for m in modes {
            let later = match m.as_str() {
                Some("launch") => "M2",
                Some("direct") => "M3",
                _ => continue,
            };
            return Err(invalid(
                "mode",
                format!("{m} is not available until {later}"),
            ));
        }
    }
    let write = raw
        .get("decision_log")
        .and_then(|d| d.get("write"))
        .and_then(toml::Value::as_str);
    if write == Some("auto") {
        return Err(invalid(
            "decision_log.write",
            "auto export is not available while two writers exist (spec §7.6)",
        ));
    }
    toml::from_str(text).map_err(parse_err)
}

fn invalid(key: &str, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        key: key.to_string(),
        message: message.into(),
    }
}

impl Config {
    fn validate(&self, env: &dyn Env) -> Result<(), ConfigError> {
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.project.iter().enumerate() {
            let key = format!("project[{i}].slug");
            if !is_valid_slug(&p.slug) {
                return Err(invalid(
                    &key,
                    format!("{:?} must match ^[a-z0-9-]+$", p.slug),
                ));
            }
            if p.slug == DISPATCH_SLUG {
                return Err(invalid(
                    &key,
                    format!("{DISPATCH_SLUG:?} is reserved for the DISPATCH line"),
                ));
            }
            if !seen.insert(p.slug.as_str()) {
                return Err(invalid(&key, format!("duplicate slug {:?}", p.slug)));
            }
        }
        let homes = [self.claude_home(env), self.codex_home(env)];
        let checks = [
            ("decision_log.path", self.decision_log_path(env)),
            ("PELHAIM_DATA_DIR", Self::data_dir(env)),
        ];
        for (key, path) in checks {
            for home in &homes {
                if is_inside(&path, home) {
                    return Err(invalid(
                        key,
                        format!(
                            "{} is inside agent home {}, which pelhaim never writes to",
                            path.display(),
                            home.display()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn engine_config(&self, local_offset: FixedOffset) -> EngineConfig {
        EngineConfig {
            projects: self
                .project
                .iter()
                .map(|p| ProjectCfg {
                    slug: p.slug.clone(),
                    name: p.name.clone(),
                    order: p.order,
                })
                .collect(),
            dispatch_sessions: self.dispatch.sessions.clone(),
            permission_after: self.observe.permission_after.0,
            stall_after: self.observe.stall_after.0,
            result_alerts: self.observe.result_alerts,
            float_alert_lines: self.board.float_alert_lines,
            keep_awake_enabled: self.keep_awake.enabled,
            include_observed: self.keep_awake.include_observed,
            remote_enabled: self.remote.enabled,
            decision_log_write: self.decision_log.write == LogWrite::OnClick,
            local_offset,
        }
    }

    pub fn claude_home(&self, env: &dyn Env) -> PathBuf {
        home_dir(
            &self.observe.claude_home,
            "CLAUDE_CONFIG_DIR",
            ".claude",
            env,
        )
    }

    pub fn codex_home(&self, env: &dyn Env) -> PathBuf {
        home_dir(&self.observe.codex_home, "CODEX_HOME", ".codex", env)
    }

    pub fn data_dir(env: &dyn Env) -> PathBuf {
        match env.var("PELHAIM_DATA_DIR") {
            Some(d) => expand_tilde(&d, env),
            None => env.home().join(".local/share/pelhaim"),
        }
    }

    pub fn decision_log_path(&self, env: &dyn Env) -> PathBuf {
        expand_tilde(&self.decision_log.path, env)
    }
}

fn home_dir(configured: &Option<String>, var: &str, default: &str, env: &dyn Env) -> PathBuf {
    match configured.clone().or_else(|| env.var(var)) {
        Some(p) => expand_tilde(&p, env),
        None => env.home().join(default),
    }
}

/// Compares lexically normalised paths, and canonical ones too when both exist,
/// so `..` segments and symlinks cannot smuggle a write path into an agent home.
fn is_inside(path: &Path, home: &Path) -> bool {
    if normalize(path).starts_with(normalize(home)) {
        return true;
    }
    match (canonical_prefix(path), home.canonicalize()) {
        (Some(p), Ok(h)) => p.starts_with(h),
        _ => false,
    }
}

/// Canonicalises the deepest existing ancestor and re-appends the rest, since
/// the data dir or log file may not exist yet.
fn canonical_prefix(path: &Path) -> Option<PathBuf> {
    let path = normalize(path);
    let mut rest = Vec::new();
    let mut cur = path.as_path();
    loop {
        if let Ok(c) = cur.canonicalize() {
            return Some(rest.iter().rev().fold(c, |acc, part| acc.join(part)));
        }
        rest.push(cur.file_name()?.to_os_string());
        cur = cur.parent()?;
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MapEnv;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/config")
            .join(name)
    }

    fn load_str(toml: &str, env: MapEnv) -> Result<Loaded, ConfigError> {
        let dir = std::env::temp_dir().join(format!(
            "pelhaim-config-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, toml).unwrap();
        let r = load(Some(&path), &env);
        std::fs::remove_dir_all(&dir).unwrap();
        r
    }

    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn err_text(toml: &str) -> String {
        load_str(toml, MapEnv::new("/tmp/h"))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn missing_config_file_yields_defaults() {
        let env = MapEnv::new("/tmp/h");
        let l = load(None, &env).unwrap();
        assert!(l.source.is_none());
        assert!(l.config.project.is_empty());
        assert_eq!(l.config.mode, vec![Mode::Observe]);
        assert_eq!(
            l.config.observe.active_window.0,
            Duration::from_secs(48 * 3600)
        );
    }

    #[test]
    fn defaults_match_engine_config_default() {
        let utc = FixedOffset::east_opt(0).unwrap();
        assert_eq!(
            Config::default().engine_config(utc),
            EngineConfig::default()
        );
    }

    #[test]
    fn empty_file_equals_defaults() {
        let l = load_str("", MapEnv::new("/tmp/h")).unwrap();
        assert_eq!(l.config, Config::default());
        assert!(l.source.is_some());
    }

    #[test]
    fn full_fixture_parses() {
        let path = fixture("full.toml");
        let l = load(Some(&path), &MapEnv::new("/tmp/h")).unwrap();
        assert_eq!(l.source.as_deref(), Some(path.as_path()));
        let c = &l.config;
        assert_eq!(c.project.len(), 7);
        let sa = &c.project[0];
        assert_eq!(sa.slug, "safeascent");
        assert_eq!(sa.name, "SafeAscent");
        assert_eq!(sa.path.as_deref(), Some("~/Developer/safeascent"));
        assert_eq!(sa.repo.as_deref(), Some("SebastianFrazier26/safeascent"));
        assert_eq!(sa.order, 1);
        assert_eq!(sa.log_section.as_deref(), Some("SafeAscent"));
        assert_eq!(c.project[6].log_section, None);
        assert_eq!(
            c.dispatch.sessions,
            vec!["11111111-2222-3333-4444-555555555555"]
        );
        assert!(!c.keep_awake.enabled);
        assert!(!c.keep_awake.include_observed);
        assert!(c.keep_awake.keep_display_awake);
        assert!(c.remote.enabled);
        assert_eq!(c.board.palette, Palette::HighContrast);
        assert_eq!(c.board.motion, Motion::Reduced);
        assert!(!c.board.float_alert_lines);
        assert_eq!(c.decision_log.write, LogWrite::Off);
        assert_eq!(c.observe.active_window.0, Duration::from_secs(24 * 3600));
        assert_eq!(c.observe.stall_after.0, Some(Duration::from_secs(15 * 60)));
        assert_eq!(c.observe.permission_after.0, None);
        assert!(c.observe.result_alerts);

        let e = c.engine_config(FixedOffset::east_opt(3600).unwrap());
        assert_eq!(e.projects.len(), 7);
        assert_eq!(
            e.projects[0],
            ProjectCfg {
                slug: "safeascent".into(),
                name: "SafeAscent".into(),
                order: 1
            }
        );
        assert_eq!(e.dispatch_sessions, c.dispatch.sessions);
        assert_eq!(e.permission_after, None);
        assert_eq!(e.stall_after, Some(Duration::from_secs(15 * 60)));
        assert!(e.result_alerts);
        assert!(!e.float_alert_lines);
        assert!(!e.keep_awake_enabled);
        assert!(!e.include_observed);
        assert!(e.remote_enabled);
        assert!(!e.decision_log_write);
        assert_eq!(e.local_offset, FixedOffset::east_opt(3600).unwrap());
    }

    #[test]
    fn unknown_key_is_error() {
        let msg = err_text("[observe]\nstal_after = \"1m\"\n");
        assert!(msg.contains("stal_after"), "{msg}");
        let msg = err_text("[[project]]\nslug = \"a\"\nname = \"A\"\ncolour = \"red\"\n");
        assert!(msg.contains("colour"), "{msg}");
        let msg = err_text("palette = \"film\"\n");
        assert!(msg.contains("palette"), "{msg}");
    }

    #[test]
    fn launch_section_rejected_with_milestone() {
        for section in ["launch", "direct", "approval", "project_overrides", "git"] {
            let msg = err_text(&format!("[{section}]\nx = 1\n"));
            assert!(
                msg.contains(&format!("section [{section}] is not supported until M2")),
                "{msg}"
            );
        }
        let msg = err_text("[project_overrides.safeascent.approval]\nnetwork = \"deny\"\n");
        assert!(msg.contains("section [project_overrides]"), "{msg}");
    }

    #[test]
    fn launch_and_direct_modes_rejected_with_milestone() {
        let msg = err_text("mode = [\"observe\", \"launch\"]\n");
        assert!(msg.contains("mode"), "{msg}");
        assert!(
            msg.contains("\"launch\" is not available until M2"),
            "{msg}"
        );
        let msg = err_text("mode = [\"direct\"]\n");
        assert!(
            msg.contains("\"direct\" is not available until M3"),
            "{msg}"
        );
        let msg = err_text("mode = [\"teleport\"]\n");
        assert!(msg.contains("teleport"), "{msg}");
    }

    #[test]
    fn auto_write_rejected() {
        let msg = err_text("[decision_log]\nwrite = \"auto\"\n");
        assert!(msg.contains("decision_log.write"), "{msg}");
        assert!(
            msg.contains("auto export is not available while two writers exist (spec §7.6)"),
            "{msg}"
        );
    }

    #[test]
    fn reserved_and_bad_slugs_rejected() {
        let msg = err_text("[[project]]\nslug = \"dispatch\"\nname = \"D\"\n");
        assert!(msg.contains("project[0].slug"), "{msg}");
        assert!(msg.contains("reserved"), "{msg}");

        let msg = err_text("[[project]]\nslug = \"Safe\"\nname = \"S\"\n");
        assert!(msg.contains("project[0].slug"), "{msg}");
        assert!(msg.contains("Safe"), "{msg}");

        let msg = err_text(
            "[[project]]\nslug = \"a\"\nname = \"A\"\n[[project]]\nslug = \"a\"\nname = \"B\"\n",
        );
        assert!(msg.contains("project[1].slug"), "{msg}");
        assert!(msg.contains("duplicate"), "{msg}");
    }

    #[test]
    fn homes_honour_env_then_default() {
        let c = Config::default();
        let bare = MapEnv::new("/tmp/h");
        assert_eq!(c.claude_home(&bare), PathBuf::from("/tmp/h/.claude"));
        assert_eq!(c.codex_home(&bare), PathBuf::from("/tmp/h/.codex"));

        let env = MapEnv::new("/tmp/h")
            .with("CLAUDE_CONFIG_DIR", "/x")
            .with("CODEX_HOME", "/y");
        assert_eq!(c.claude_home(&env), PathBuf::from("/x"));
        assert_eq!(c.codex_home(&env), PathBuf::from("/y"));

        let mut c = Config::default();
        c.observe.claude_home = Some("~/cfg-claude".into());
        c.observe.codex_home = Some("/cfg-codex".into());
        assert_eq!(c.claude_home(&env), PathBuf::from("/tmp/h/cfg-claude"));
        assert_eq!(c.codex_home(&env), PathBuf::from("/cfg-codex"));
    }

    #[test]
    fn data_dir_and_decision_log_paths() {
        let env = MapEnv::new("/tmp/h");
        assert_eq!(
            Config::data_dir(&env),
            PathBuf::from("/tmp/h/.local/share/pelhaim")
        );
        assert_eq!(
            Config::data_dir(&env.with("PELHAIM_DATA_DIR", "/d")),
            PathBuf::from("/d")
        );
        assert_eq!(
            Config::default().decision_log_path(&MapEnv::new("/tmp/h")),
            PathBuf::from("/tmp/h/Developer/DECISION-LOG.md")
        );
    }

    #[test]
    fn duration_parse() {
        assert_eq!(
            parse_duration("20s").unwrap(),
            Some(Duration::from_secs(20))
        );
        assert_eq!(
            parse_duration("10m").unwrap(),
            Some(Duration::from_secs(600))
        );
        assert_eq!(
            parse_duration("48h").unwrap(),
            Some(Duration::from_secs(48 * 3600))
        );
        assert_eq!(parse_duration("off").unwrap(), None);
        for bad in [
            "5d",
            "",
            "s",
            "-1s",
            "+1s",
            "1.5h",
            " 1s",
            "1 s",
            "OFF",
            "99999999999999999999h",
        ] {
            assert!(parse_duration(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn bad_duration_values_rejected() {
        let msg = err_text("[observe]\nstall_after = \"5d\"\n");
        assert!(msg.contains("5d"), "{msg}");
        let msg = err_text("[observe]\nactive_window = \"off\"\n");
        assert!(msg.contains("off"), "{msg}");
        let l = load_str(
            "[observe]\nstall_after = \"off\"\npermission_after = \"30s\"\n",
            MapEnv::new("/tmp/h"),
        )
        .unwrap();
        assert_eq!(l.config.observe.stall_after.0, None);
        assert_eq!(
            l.config.observe.permission_after.0,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn data_dir_inside_claude_home_rejected() {
        let env = MapEnv::new("/tmp/h").with("PELHAIM_DATA_DIR", "/tmp/h/.claude/pelhaim");
        let msg = load(None, &env).unwrap_err().to_string();
        assert!(msg.contains("PELHAIM_DATA_DIR"), "{msg}");

        let env = MapEnv::new("/tmp/h")
            .with("CODEX_HOME", "/c")
            .with("PELHAIM_DATA_DIR", "/c/x/../y");
        let msg = load(None, &env).unwrap_err().to_string();
        assert!(msg.contains("PELHAIM_DATA_DIR"), "{msg}");

        let env = MapEnv::new("/tmp/h").with("PELHAIM_DATA_DIR", "/tmp/h/.claudex");
        assert!(load(None, &env).is_ok());
    }

    #[test]
    fn decision_log_inside_codex_home_rejected() {
        let msg = err_text("[decision_log]\npath = \"~/.codex/DECISION-LOG.md\"\n");
        assert!(msg.contains("decision_log.path"), "{msg}");
        let msg = err_text(
            "[observe]\nclaude_home = \"/agents/claude\"\n[decision_log]\npath = \"/agents/claude/log.md\"\n",
        );
        assert!(msg.contains("decision_log.path"), "{msg}");
    }

    #[test]
    fn lookup_order_explicit_then_env_then_default() {
        let missing = Path::new("/nonexistent/pelhaim.toml");
        let err = load(Some(missing), &MapEnv::new("/tmp/h")).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }), "{err}");

        let full = fixture("full.toml");
        let env = MapEnv::new("/tmp/h").with("PELHAIM_CONFIG", full.to_str().unwrap());
        let l = load(None, &env).unwrap();
        assert_eq!(l.source.as_deref(), Some(full.as_path()));

        let env = MapEnv::new("/tmp/h").with("PELHAIM_CONFIG", "/nonexistent/env.toml");
        let l = load(Some(&full), &env).unwrap();
        assert_eq!(l.source.as_deref(), Some(full.as_path()));

        let err = load(None, &env).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }), "{err}");

        let home = fixture("home");
        let l = load(None, &MapEnv::new(&home)).unwrap();
        assert_eq!(l.source, Some(home.join(".config/pelhaim/config.toml")));
        assert_eq!(l.config.project.len(), 1);
    }
}
