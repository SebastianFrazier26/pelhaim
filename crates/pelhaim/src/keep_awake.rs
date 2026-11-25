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
            // 2026-11-24: unverified on a real Linux host. There is no `-w`, so the inhibited
            // command polls pelhaim's pid itself: POSIX `sh` + `kill -0` works on every
            // distro, unlike GNU-only `tail --pid`. The 5 s poll bounds how long a
            // SIGKILLed pelhaim keeps the machine awake. Its own process group lets
            // Release take down the poll loop and its `sleep` along with systemd-inhibit.
            // `keep_display` is not passed: `idle` already covers display blanking.
            Mechanism::SystemdInhibit(bin) => {
                let mut c = Command::new(bin);
                c.args([
                    "--what=sleep:idle",
                    "--who=pelhaim",
                    "--why=agents running",
                    "sh",
                    "-c",
                    INHIBIT_WAIT_SCRIPT,
                    "pelhaim-wait",
                ])
                .arg(self.pid.to_string());
                #[cfg(unix)]
                std::os::unix::process::CommandExt::process_group(&mut c, 0);
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
        match self.mechanism {
            #[cfg(unix)]
            Some(Mechanism::SystemdInhibit(_)) => kill_group(&mut c),
            _ => {
                if c.try_wait()?.is_none() {
                    c.kill()?;
                    c.wait()?;
                }
                Ok(())
            }
        }
    }
}

const INHIBIT_WAIT_SCRIPT: &str = "while kill -0 \"$1\" 2>/dev/null; do sleep 5; done";

/// Signals the group even when the leader already exited, since its descendants
/// may outlive it. SIGKILL follows if the group ignores SIGTERM, so Drop never hangs.
#[cfg(unix)]
fn kill_group(c: &mut Child) -> std::io::Result<()> {
    use rustix::process::{Pid, Signal, kill_process_group};
    use std::time::{Duration, Instant};

    let pgid = Pid::from_child(c);
    let signal = |sig| match kill_process_group(pgid, sig) {
        Err(rustix::io::Errno::SRCH) => Ok(()),
        r => r.map_err(std::io::Error::from),
    };
    signal(Signal::TERM)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while c.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            signal(Signal::KILL)?;
            c.wait()?;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
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

        /// Linux refuses to exec a file that any process holds open for writing (ETXTBSY), and a
        /// sibling test thread that forks while `fs::write` has the script open carries that
        /// descriptor into its child until the child's exec. So the script is written under
        /// another name and copied into place by `cp`, whose write descriptor no fork of ours
        /// can inherit.
        fn write_script(p: &Path, body: &str) {
            let src = p.with_extension("src");
            std::fs::write(&src, body).unwrap();
            let ok = std::process::Command::new("cp")
                .arg(&src)
                .arg(p)
                .status()
                .unwrap()
                .success();
            assert!(ok, "cp {} {}", src.display(), p.display());
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        /// F42: one argv element per line. The rename makes argv.txt appear complete,
        /// and `exec` keeps `$$` equal to the pid the holder owns.
        pub fn install(dir: &Path, name: &str) {
            write_script(
                &dir.join(name),
                "#!/bin/sh\nd=\"$(dirname \"$0\")\"\necho run >> \"$d/runs.txt\"\n\
                 echo $$ > \"$d/pid.tmp\" && mv \"$d/pid.tmp\" \"$d/pid.txt\"\n\
                 printf '%s\\n' \"$@\" > \"$d/argv.tmp\" && mv \"$d/argv.tmp\" \"$d/argv.txt\"\n\
                 exec sleep 600\n",
            );
        }

        /// Like systemd-inhibit: skips its `--` options and runs the rest as a child, so
        /// the real `sh -c` pid-poll loop runs as a grandchild in the holder's group.
        pub fn install_inhibit(dir: &Path) {
            write_script(
                &dir.join("systemd-inhibit"),
                "#!/bin/sh\nd=\"$(dirname \"$0\")\"\n\
                 echo $$ > \"$d/pid.tmp\" && mv \"$d/pid.tmp\" \"$d/pid.txt\"\n\
                 while [ \"${1#--}\" != \"$1\" ]; do shift; done\n\
                 \"$@\" &\nwait $!\n",
            );
        }

        pub fn group(pgid: &str) -> Vec<String> {
            let out = std::process::Command::new("pgrep")
                .args(["-g", pgid.trim()])
                .output()
                .unwrap();
            String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .map(str::to_string)
                .collect()
        }

        pub fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ok() {
                assert!(Instant::now() < deadline, "timed out: {what}");
                std::thread::sleep(Duration::from_millis(10));
            }
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
                "sh",
                "-c",
                INHIBIT_WAIT_SCRIPT,
                "pelhaim-wait",
                "4242"
            ]
        );
    }

    #[cfg(unix)]
    fn inhibit_holder(dir: &Path, pid: u32) -> (Holder, String) {
        fake::install_inhibit(dir);
        let m = detect(dir.as_os_str(), Os::Linux).unwrap();
        let mut h = Holder::new(Some(m), pid, false);
        h.apply(KeepAwakeAction::Hold).unwrap();
        let pgid = fake::wait_for(dir.join("pid.txt")).trim().to_string();
        (h, pgid)
    }

    #[cfg(unix)]
    #[test]
    fn inhibit_release_kills_whole_group() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, pgid) = inhibit_holder(dir.path(), std::process::id());
        fake::eventually("grandchild started", || fake::group(&pgid).len() >= 2);
        h.apply(KeepAwakeAction::Release).unwrap();
        assert!(!h.is_held());
        fake::eventually("group gone after Release", || fake::group(&pgid).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn inhibit_drop_kills_whole_group() {
        let dir = tempfile::tempdir().unwrap();
        let (h, pgid) = inhibit_holder(dir.path(), std::process::id());
        fake::eventually("grandchild started", || fake::group(&pgid).len() >= 2);
        drop(h);
        fake::eventually("group gone after Drop", || fake::group(&pgid).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn inhibit_ends_when_pelhaim_pid_is_gone() {
        let mut gone = std::process::Command::new("true").spawn().unwrap();
        let gone_pid = gone.id();
        gone.wait().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (mut h, pgid) = inhibit_holder(dir.path(), gone_pid);
        fake::eventually("inhibitor exits on its own", || {
            h.child.as_mut().unwrap().try_wait().unwrap().is_some()
        });
        fake::eventually("group gone", || fake::group(&pgid).is_empty());
        h.apply(KeepAwakeAction::Release).unwrap();
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
