use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pelhaim_adapters::observe::{CLAUDE_TESTED, CODEX_TESTED, is_untested};
use pelhaim_core::Event;

use crate::config::{Config, ConfigError, Loaded};
use crate::env::Env;
use crate::keep_awake::detect;
use crate::run::{CATCH_UP_TIMEOUT, adapters, catch_up, keep_awake_search, runtime, this_os};

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// One line per check. Prints paths, versions and session ids only, never a value from
/// the environment or a transcript.
pub fn doctor(cfg: &Loaded, env: &dyn Env) -> String {
    let config = match &cfg.source {
        Some(p) => format!("config: {} ok", p.display()),
        None => "config: defaults ok".to_string(),
    };
    report(&cfg.config, config, env)
}

/// The report for a config that failed to load: the error, then every other check run
/// against the defaults.
pub fn doctor_config_error(err: &ConfigError, env: &dyn Env) -> String {
    report(&Config::default(), format!("config: error: {err}"), env)
}

fn report(cfg: &Config, config_line: String, env: &dyn Env) -> String {
    let lines = [
        cli_line("claude", CLAUDE_TESTED, env),
        cli_line("codex", CODEX_TESTED, env),
        config_line,
        data_dir_line(&Config::data_dir(env)),
        decision_log_line(&cfg.decision_log_path(env)),
        keep_awake_line(cfg, env),
        format!(
            "gpu: {}",
            pelhaim_gpu::adapter_info().unwrap_or_else(|| "none".into())
        ),
        untested_line(cfg, env),
    ];
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn highest(tested: &[&str]) -> String {
    tested
        .iter()
        .find(|t| !tested.iter().any(|o| is_untested(o, &[t])))
        .map_or_else(|| "none".into(), |t| t.to_string())
}

fn cli_line(name: &str, tested: &[&str], env: &dyn Env) -> String {
    let found = find_on_path(name, env)
        .and_then(|bin| version_of(&bin))
        .unwrap_or_else(|| "not found".into());
    format!("{name}: {found} (tested {})", highest(tested))
}

fn find_on_path(name: &str, env: &dyn Env) -> Option<PathBuf> {
    let path = env.var("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Runs `<bin> --version` with a timeout; that only reads. The first token shaped like a
/// version wins: `2.1.331 (Claude Code)`, `codex-cli 0.158.0`.
fn version_of(bin: &Path) -> Option<String> {
    let mut child = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    parse_version(&out)
}

fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace()
        .find(|t| {
            t.starts_with(|c: char| c.is_ascii_digit())
                && t.contains('.')
                && t.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(str::to_string)
}

fn data_dir_line(dir: &Path) -> String {
    let ok = std::fs::create_dir_all(dir).is_ok() && tempfile_probe(dir);
    let state = if ok { "writable" } else { "not writable" };
    format!("data dir: {} {state}", dir.display())
}

fn tempfile_probe(dir: &Path) -> bool {
    let probe = dir.join(format!(".pelhaim-doctor-{}", std::process::id()));
    let ok = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Never opens the log for writing: the owner's real file has a second writer (§7.6).
fn decision_log_line(path: &Path) -> String {
    let state = match std::fs::metadata(path) {
        Ok(m) if m.permissions().readonly() => "not writable",
        Ok(_) => "found",
        Err(_) => "not found",
    };
    format!("decision log: {} {state}", path.display())
}

fn keep_awake_line(cfg: &Config, env: &dyn Env) -> String {
    let os = this_os();
    let name = match detect(&keep_awake_search(os, env), os) {
        Some(crate::keep_awake::Mechanism::Caffeinate(_)) => "caffeinate",
        Some(crate::keep_awake::Mechanism::SystemdInhibit(_)) => "systemd-inhibit",
        None => "none",
    };
    if cfg.keep_awake.enabled {
        format!("keep awake: {name}")
    } else {
        format!("keep awake: {name} (disabled in config)")
    }
}

/// Explains the ⚠ tick: sessions whose CLI version is newer than any tested one.
fn untested_line(cfg: &Config, env: &dyn Env) -> String {
    let events = runtime()
        .map_err(anyhow::Error::from)
        .and_then(|rt| rt.block_on(catch_up(adapters(cfg, env, true), CATCH_UP_TIMEOUT)));
    match events {
        Ok(events) => {
            let ids: BTreeSet<&str> = events
                .iter()
                .filter_map(|e| match e {
                    Event::FormatUntested { key, .. } => Some(key.session_id()),
                    _ => None,
                })
                .collect();
            if ids.is_empty() {
                "format untested: none".into()
            } else {
                format!(
                    "format untested: {}",
                    ids.into_iter().collect::<Vec<_>>().join(", ")
                )
            }
        }
        Err(e) => format!("format untested: unknown ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(
            parse_version("2.1.331 (Claude Code)\n").as_deref(),
            Some("2.1.331")
        );
        assert_eq!(
            parse_version("codex-cli 0.162.1").as_deref(),
            Some("0.162.1")
        );
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("v1"), None);
    }

    #[test]
    fn highest_tested_version() {
        assert_eq!(highest(CODEX_TESTED), "0.162.1");
        assert_eq!(highest(&["0.162.1", "0.158.0"]), "0.162.1");
        assert_eq!(highest(CLAUDE_TESTED), "2.1.332");
    }
}
