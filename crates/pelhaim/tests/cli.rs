#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

use pelhaim_core::{Decision, DecisionId, Origin, OwnerEvent, Who};
use pelhaim_store::Store;
use sha2::{Digest, Sha256};

const SECRET: &str = "sk-test-not-a-real-key-1234";

/// Everything the child sees lives in one tempdir; the parent's env is never changed.
struct Rig {
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        let rig = Rig {
            dir: tempfile::tempdir().unwrap(),
        };
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../pelhaim-adapters/tests/fixtures");
        copy_tree(&fixtures.join("claude"), &rig.claude());
        copy_tree(&fixtures.join("codex"), &rig.codex());
        // F25: checked-in fixtures fall outside the 48 h active window, so the copies are touched to now.
        let now = SystemTime::now();
        for f in files_under(&rig.claude())
            .into_iter()
            .chain(files_under(&rig.codex()))
        {
            File::options()
                .append(true)
                .open(&f)
                .unwrap()
                .set_modified(now)
                .unwrap();
        }
        fs::create_dir_all(rig.home().join("Developer")).unwrap();
        fs::create_dir_all(rig.bin()).unwrap();
        fake_bin(&rig.bin().join("claude"), "2.1.331 (Claude Code)");
        fake_bin(&rig.bin().join("codex"), "codex-cli 0.158.0");
        rig
    }

    fn p(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn claude(&self) -> PathBuf {
        self.p("claude-home")
    }
    fn codex(&self) -> PathBuf {
        self.p("codex-home")
    }
    fn data(&self) -> PathBuf {
        self.p("data")
    }
    fn home(&self) -> PathBuf {
        self.p("home")
    }
    fn bin(&self) -> PathBuf {
        self.p("bin")
    }
    fn log(&self) -> PathBuf {
        self.home().join("Developer/DECISION-LOG.md")
    }

    fn run(&self, args: &[&str]) -> Output {
        let config =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config/fixture.toml");
        Command::new(env!("CARGO_BIN_EXE_pelhaim"))
            .args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", self.bin())
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .env("CODEX_HOME", self.codex())
            .env("PELHAIM_DATA_DIR", self.data())
            .env("PELHAIM_CONFIG", config)
            .env("ANTHROPIC_API_KEY", SECRET)
            .output()
            .unwrap()
    }

    /// (len, mtime, sha256) of every file in both agent homes.
    fn homes_state(&self) -> BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)> {
        files_under(&self.claude())
            .into_iter()
            .chain(files_under(&self.codex()))
            .map(|f| {
                let m = fs::metadata(&f).unwrap();
                let hash = Sha256::digest(fs::read(&f).unwrap()).to_vec();
                (f, (m.len(), m.modified().unwrap(), hash))
            })
            .collect()
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn fake_bin(path: &Path, version: &str) {
    fs::write(path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn read_dump(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn doctor_reports_versions_and_never_values() {
    let rig = Rig::new();
    let out = rig.run(&["doctor"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}\n{}", stderr(&out));
    assert!(text.contains("claude: 2.1.331 (tested 2.1.332)"), "{text}");
    assert!(text.contains("codex: 0.158.0 (tested 0.162.1)"), "{text}");
    assert!(text.contains("format untested: S3\n"), "{text}");
    assert!(text.contains("config: "), "{text}");
    assert!(text.contains("fixture.toml ok"), "{text}");
    assert!(
        text.contains(&format!("data dir: {} writable", rig.data().display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("decision log: {} not found", rig.log().display())),
        "{text}"
    );
    assert!(text.contains("keep awake: "), "{text}");
    assert!(text.contains("gpu: "), "{text}");
    assert!(!text.contains(SECRET), "{text}");
    assert!(
        !text.contains("Fix the atlas"),
        "transcript text leaked: {text}"
    );

    fs::remove_file(rig.bin().join("codex")).unwrap();
    let text = stdout(&rig.run(&["doctor"]));
    assert!(text.contains("codex: not found (tested 0.162.1)"), "{text}");
}

#[test]
fn dump_scene_writes_display_list_json_and_leaves_homes_untouched() {
    let rig = Rig::new();
    let before = rig.homes_state();
    let out_path = rig.p("scene.json");
    let out = rig.run(&["--headless", "--dump-scene", out_path.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(rig.homes_state(), before, "an agent home changed");

    let dump = read_dump(&out_path);
    let prims = dump["prims"].as_array().unwrap();
    assert!(!prims.is_empty());
    let row = dump["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["key"] == "Dispatch")
        .expect("a DISPATCH line")["label_row"]
        .clone();
    let (x0, y0) = (row["x"].as_f64().unwrap(), row["y"].as_f64().unwrap());
    let (x1, y1) = (
        x0 + row["w"].as_f64().unwrap(),
        y0 + row["h"].as_f64().unwrap(),
    );
    // F52: matte `label` #B4B4B7 glyph pixels inside the DISPATCH label row.
    let label = serde_json::json!([0xB4, 0xB4, 0xB7, 0xFF]);
    let inside = prims.iter().filter_map(|p| p.get("Rect")).any(|r| {
        let b = &r["r"];
        let (x, y) = (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap());
        r["color"] == label && x >= x0 && x < x1 && y >= y0 && y < y1
    });
    assert!(
        inside,
        "no label-coloured rect in the DISPATCH label row {row}"
    );
    assert!(rig.data().join("pelhaim.db").exists());
    assert!(rig.data().join("logs/pelhaim.log").exists());
}

fn decision(id: i64, project: &str, title: &str) -> Decision {
    Decision {
        id: DecisionId(id),
        project: project.into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
        title: title.into(),
        why: "Because the fixture says so".into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: None,
        written_to_md: false,
        project_name: None,
    }
}

#[test]
fn decisions_prints_rows_and_open_list() {
    let rig = Rig::new();
    let empty = rig.run(&["decisions"]);
    assert!(empty.status.success(), "{}", stderr(&empty));
    assert!(
        stdout(&empty).contains("no decisions logged"),
        "{}",
        stdout(&empty)
    );
    assert!(
        !rig.data().join("pelhaim.db").exists(),
        "decisions created the store"
    );

    let mut store = Store::open(&rig.data().join("pelhaim.db")).unwrap();
    let at = chrono::Utc::now();
    store
        .append(
            &OwnerEvent::DecisionLogged(decision(1, "ethica", "Use a held-out judge")),
            at,
        )
        .unwrap();
    store
        .append(
            &OwnerEvent::DecisionLogged(decision(2, "cabn", "Pixel font only")),
            at,
        )
        .unwrap();
    store
        .append(&OwnerEvent::DecisionExported { id: DecisionId(1) }, at)
        .unwrap();
    drop(store);

    let out = rig.run(&["decisions"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{}", stderr(&out));
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("2026-11-24"))
        .collect();
    assert_eq!(rows.len(), 2, "{text}");
    assert!(
        rows[0].contains("cabn") && rows[0].contains("Pixel font only"),
        "{text}"
    );
    assert!(rows[0].contains("not exported"), "{text}");
    assert!(
        rows[1].contains("ethica") && rows[1].contains("Use a held-out judge"),
        "{text}"
    );
    assert!(
        rows[1].contains("exported") && !rows[1].contains("not exported"),
        "{text}"
    );
    assert!(text.contains("Why: Because the fixture says so"), "{text}");

    let missing = rig.run(&["decisions", "--open"]);
    assert!(
        stdout(&missing).contains("not found"),
        "{}",
        stdout(&missing)
    );

    fs::write(
        rig.log(),
        "# Decision log\n\n## ethica\n\n### Open decisions (need you)\n\n- Pick a judge\n- Corpus size\n\n### Decided\n\n---\n\n## cabn\n\n### Decided\n\n---\n\n## Standing rules\n",
    )
    .unwrap();
    let out = rig.run(&["decisions", "--open"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(text.contains("ethica (2)"), "{text}");
    assert!(text.contains("  - Pick a judge\n  - Corpus size"), "{text}");
    assert!(
        !text.contains("cabn"),
        "projects with nothing open are skipped: {text}"
    );
}

#[test]
fn launch_mode_in_config_rejected_with_milestone_message() {
    let rig = Rig::new();
    for (name, body) in [
        ("mode.toml", "mode = [\"observe\", \"launch\"]\n"),
        ("section.toml", "[launch]\nclaude_bin = \"claude\"\n"),
    ] {
        let path = rig.p(name);
        fs::write(&path, body).unwrap();
        let out = rig.run(&["--config", path.to_str().unwrap()]);
        assert!(!out.status.success());
        assert!(stderr(&out).contains("M2"), "{}", stderr(&out));

        let out = rig.run(&["--config", path.to_str().unwrap(), "doctor"]);
        assert!(!out.status.success());
        let text = stdout(&out);
        assert!(text.contains("config: ") && text.contains("M2"), "{text}");
        assert!(
            text.contains("claude: 2.1.331"),
            "doctor still reports the rest: {text}"
        );
    }
}

#[test]
fn bare_and_observe_accept_no_extra_modes() {
    let rig = Rig::new();
    for args in [
        &["launch"][..],
        &["plan", "--project", "cabn", "goal"],
        &["direct"],
        &["observe", "--tool", "codex"],
    ] {
        let out = rig.run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains("error"), "{args:?}: {}", stderr(&out));
    }

    let out = rig.run(&["--dump-scene", rig.p("x.json").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--headless"), "{}", stderr(&out));
    let out = rig.run(&["--headless"]);
    assert_eq!(out.status.code(), Some(2));
    let out = rig.run(&["--headless", "--dump-scene", "x.json", "doctor"]);
    assert_eq!(out.status.code(), Some(2));

    for args in [&[][..], &["observe"]] {
        let path = rig.p("both.json");
        let mut full = vec!["--headless", "--dump-scene", path.to_str().unwrap()];
        full.extend_from_slice(args);
        let out = rig.run(&full);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(!read_dump(&path)["prims"].as_array().unwrap().is_empty());
        fs::remove_file(&path).unwrap();
    }
}
