use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use pelhaim_core::KeepAwakeAction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mechanism {
    Caffeinate(PathBuf),
    SystemdInhibit(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Other,
}

pub fn detect(search_path: &OsStr, os: Os) -> Option<Mechanism> {
    let (name, wrap): (&str, fn(PathBuf) -> Mechanism) = match os {
        Os::MacOs => ("caffeinate", Mechanism::Caffeinate),
        Os::Linux => ("systemd-inhibit", Mechanism::SystemdInhibit),
        Os::Other => return None,
    };
    std::env::split_paths(search_path)
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
        .map(wrap)
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

pub struct Holder {
    mechanism: Option<Mechanism>,
    pid: u32,
    keep_display: bool,
    child: Option<Child>,
}

impl Holder {
    pub fn new(m: Option<Mechanism>, pid: u32, keep_display: bool) -> Holder {
        Holder {
            mechanism: m,
            pid,
            keep_display,
            child: None,
        }
    }

    pub fn apply(&mut self, a: KeepAwakeAction) -> std::io::Result<()> {
        match a {
            KeepAwakeAction::Hold => self.hold(),
            KeepAwakeAction::Release => self.release(),
        }
    }

    pub fn is_held(&self) -> bool {
        self.child.is_some()
    }

    pub fn mechanism_name(&self) -> Option<&'static str> {
        self.mechanism.as_ref().map(|m| match m {
            Mechanism::Caffeinate(_) => "caffeinate",
            Mechanism::SystemdInhibit(_) => "systemd-inhibit",
        })
    }

    fn hold(&mut self) -> std::io::Result<()> {
        if let Some(c) = &mut self.child {
            // A child that died on its own (crash, external kill) is respawned rather
            // than silently counted as held.
            if c.try_wait()?.is_none() {
                return Ok(());
            }
            self.child = None;
        }
        let Some(m) = &self.mechanism else {
            return Ok(());
        };
        let mut cmd = match m {
            Mechanism::Caffeinate(bin) => {
                let mut c = Command::new(bin);
                c.args(["-i", "-m", "-s"]);
                if self.keep_display {
                    c.arg("-d");
                }
                c.arg("-w").arg(self.pid.to_string());
                c
            }
            // 2026-11-24: unverified on a real Linux host. Unlike caffeinate's `-w`,
            // nothing ties this child's lifetime to pelhaim's pid, so a SIGKILLed
            // pelhaim leaves it running.
            Mechanism::SystemdInhibit(bin) => {
                let mut c = Command::new(bin);
                c.args([
                    "--what=sleep:idle",
                    "--who=pelhaim",
                    "--why=agents running",
                    "sleep",
                    "infinity",
                ]);
                c
            }
        };
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        Ok(())
    }

    fn release(&mut self) -> std::io::Result<()> {
        let Some(mut c) = self.child.take() else {
            return Ok(());
        };
        if c.try_wait()?.is_none() {
            c.kill()?;
            c.wait()?;
        }
        Ok(())
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Reads `pmset -g ps`; `Some(true)` means on battery.
pub fn parse_pmset_ps(out: &str) -> Option<bool> {
    let first = out.lines().next()?;
    if first.contains("'Battery Power'") {
        Some(true)
    } else if first.contains("'AC Power'") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    mod fake {
        use std::os::unix::fs::PermissionsExt;
        use std::path::{Path, PathBuf};
        use std::time::{Duration, Instant};

        /// F42: one argv element per line. The rename makes argv.txt appear complete,
        /// and `exec` keeps `$$` equal to the pid the holder owns.
        pub fn install(dir: &Path, name: &str) {
            let p = dir.join(name);
            std::fs::write(
                &p,
                "#!/bin/sh\nd=\"$(dirname \"$0\")\"\necho run >> \"$d/runs.txt\"\n\
                 echo $$ > \"$d/pid.tmp\" && mv \"$d/pid.tmp\" \"$d/pid.txt\"\n\
                 printf '%s\\n' \"$@\" > \"$d/argv.tmp\" && mv \"$d/argv.tmp\" \"$d/argv.txt\"\n\
                 exec sleep 600\n",
            )
            .unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        pub fn wait_for(p: PathBuf) -> String {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    return s;
                }
                assert!(
                    Instant::now() < deadline,
                    "fake never wrote {}",
                    p.display()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn argv(dir: &Path) -> Vec<String> {
            wait_for(dir.join("argv.txt"))
                .lines()
                .map(str::to_string)
                .collect()
        }

        pub fn alive(pid: &str) -> bool {
            std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        }
    }

    #[cfg(unix)]
    fn mac_holder(dir: &Path, keep_display: bool) -> Holder {
        fake::install(dir, "caffeinate");
        let m = detect(dir.as_os_str(), Os::MacOs).expect("fake caffeinate on search path");
        assert_eq!(m, Mechanism::Caffeinate(dir.join("caffeinate")));
        Holder::new(Some(m), 4242, keep_display)
    }

    #[cfg(unix)]
    #[test]
    fn hold_spawns_caffeinate_with_exact_argv() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = mac_holder(dir.path(), false);
        assert_eq!(h.mechanism_name(), Some("caffeinate"));
        h.apply(KeepAwakeAction::Hold).unwrap();
        assert!(h.is_held());
        assert_eq!(fake::argv(dir.path()), ["-i", "-m", "-s", "-w", "4242"]);
    }

    #[cfg(unix)]
    #[test]
    fn display_flag_only_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = mac_holder(dir.path(), true);
        h.apply(KeepAwakeAction::Hold).unwrap();
        assert_eq!(
            fake::argv(dir.path()),
            ["-i", "-m", "-s", "-d", "-w", "4242"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn release_kills_child() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = mac_holder(dir.path(), false);
        h.apply(KeepAwakeAction::Hold).unwrap();
        let pid = fake::wait_for(dir.path().join("pid.txt"));
        assert!(fake::alive(&pid));
        h.apply(KeepAwakeAction::Release).unwrap();
        assert!(!h.is_held());
        assert!(!fake::alive(&pid), "child {pid} survived Release");
        h.apply(KeepAwakeAction::Release).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn drop_kills_child() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = mac_holder(dir.path(), false);
        h.apply(KeepAwakeAction::Hold).unwrap();
        let pid = fake::wait_for(dir.path().join("pid.txt"));
        drop(h);
        assert!(!fake::alive(&pid), "child {pid} survived Drop");
    }

    #[cfg(unix)]
    #[test]
    fn double_hold_is_single_child() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = mac_holder(dir.path(), false);
        h.apply(KeepAwakeAction::Hold).unwrap();
        fake::wait_for(dir.path().join("argv.txt"));
        h.apply(KeepAwakeAction::Hold).unwrap();
        assert!(h.is_held());
        let runs = std::fs::read_to_string(dir.path().join("runs.txt")).unwrap();
        assert_eq!(runs.lines().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn systemd_inhibit_argv_on_linux() {
        let dir = tempfile::tempdir().unwrap();
        fake::install(dir.path(), "systemd-inhibit");
        fake::install(dir.path(), "caffeinate");
        let m = detect(dir.path().as_os_str(), Os::Linux).unwrap();
        assert_eq!(
            m,
            Mechanism::SystemdInhibit(dir.path().join("systemd-inhibit"))
        );
        let mut h = Holder::new(Some(m), 4242, true);
        assert_eq!(h.mechanism_name(), Some("systemd-inhibit"));
        h.apply(KeepAwakeAction::Hold).unwrap();
        assert_eq!(
            fake::argv(dir.path()),
            [
                "--what=sleep:idle",
                "--who=pelhaim",
                "--why=agents running",
                "sleep",
                "infinity"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn detect_finds_nothing_without_binary_or_on_other_os() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect(dir.path().as_os_str(), Os::MacOs), None);
        assert_eq!(detect(dir.path().as_os_str(), Os::Linux), None);
        fake::install(dir.path(), "caffeinate");
        assert_eq!(detect(dir.path().as_os_str(), Os::Other), None);
        assert_eq!(detect(dir.path().as_os_str(), Os::Linux), None);
        std::fs::write(dir.path().join("systemd-inhibit"), "not executable").unwrap();
        assert_eq!(detect(dir.path().as_os_str(), Os::Linux), None);
    }

    #[test]
    fn no_mechanism_is_noop() {
        let mut h = Holder::new(None, 4242, true);
        assert_eq!(h.mechanism_name(), None);
        h.apply(KeepAwakeAction::Hold).unwrap();
        assert!(!h.is_held());
        h.apply(KeepAwakeAction::Release).unwrap();
        assert!(!h.is_held());
    }

    #[test]
    fn pmset_parse() {
        assert_eq!(
            parse_pmset_ps("Now drawing from 'AC Power'\n -InternalBattery-0"),
            Some(false)
        );
        assert_eq!(
            parse_pmset_ps("Now drawing from 'Battery Power'"),
            Some(true)
        );
        assert_eq!(parse_pmset_ps(""), None);
        assert_eq!(parse_pmset_ps("garbage\nNow drawing from 'AC Power'"), None);
    }
}
