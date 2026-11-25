use std::io::Write as _;
use std::path::{Path, PathBuf};

fn sh_quote(out: &mut Vec<u8>, arg: &[u8]) {
    out.push(b'\'');
    for &b in arg {
        if b == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(b);
        }
    }
    out.push(b'\'');
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes()
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> &[u8] {
    p.to_str().unwrap_or_default().as_bytes()
}

#[derive(Debug)]
pub enum ScriptError {
    BadName(String),
    EmptyArgv,
    Io(std::io::Error),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadName(n) => write!(f, "script name {n:?} is not a plain file name"),
            Self::EmptyArgv => write!(f, "script has no command to run"),
            Self::Io(e) => write!(f, "writing script: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ScriptError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `<dir>/<name>.command`: macOS opens that extension in Terminal.
pub fn write_script(
    dir: &Path,
    name: &str,
    cwd: &Path,
    argv: &[String],
) -> Result<PathBuf, ScriptError> {
    if !plain_file_name(name) {
        return Err(ScriptError::BadName(name.to_string()));
    }
    if argv.is_empty() {
        return Err(ScriptError::EmptyArgv);
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.command"));
    let mut body = b"#!/bin/sh\ncd ".to_vec();
    sh_quote(&mut body, path_bytes(cwd));
    body.extend_from_slice(b" || exit 1\nexec");
    for arg in argv {
        body.push(b' ');
        sh_quote(&mut body, arg.as_bytes());
    }
    body.push(b'\n');

    // `remove_file` unlinks a symlink itself, never its target, and `create_new` (O_EXCL)
    // refuses any entry that reappears in between, so the write never follows a link.
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o700);
    }
    let mut f = opts.open(&path)?;
    // The umask can strip bits from the creation mode.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    }
    f.write_all(&body)?;
    Ok(path)
}

pub fn open_argv(template: &[String], script: &Path) -> Vec<String> {
    let mut argv = template.to_vec();
    argv.push(script.to_string_lossy().into_owned());
    argv
}

/// `$VISUAL` / `$EDITOR` often carry flags (`code --wait`), so they are split into words (F41).
pub fn editor_argv(visual: Option<&str>, editor: Option<&str>, file: &Path) -> Vec<String> {
    let words: Vec<String> = [visual, editor]
        .into_iter()
        .flatten()
        .map(|v| {
            v.split_ascii_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .find(|w| !w.is_empty())
        .unwrap_or_else(|| vec!["vi".to_string()]);
    let mut argv = words;
    argv.push(file.to_string_lossy().into_owned());
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn script_quotes_argv() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = Path::new("/tmp/it's here");
        let path = write_script(
            dir.path(),
            "take-over",
            cwd,
            &strings(&["claude", "--resume", "a'b", "two words"]),
        )
        .unwrap();
        assert_eq!(path, dir.path().join("take-over.command"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "#!/bin/sh\ncd '/tmp/it'\\''s here' || exit 1\nexec 'claude' '--resume' 'a'\\''b' 'two words'\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[test]
    fn rewritten_script_keeps_mode_0700() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.command");
        std::fs::write(&p, "old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        write_script(dir.path(), "x", Path::new("/"), &strings(&["true"])).unwrap();
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .ends_with("exec 'true'\n")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[test]
    fn script_name_must_be_plain() {
        let dir = tempfile::tempdir().unwrap();
        let argv = strings(&["true"]);
        for name in ["../x", "a/b", ".", "..", "", "a b", "/abs"] {
            let r = write_script(dir.path(), name, Path::new("/"), &argv);
            assert!(
                matches!(r, Err(ScriptError::BadName(ref n)) if n == name),
                "{name:?}"
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(write_script(dir.path(), "take-over_1.v2", Path::new("/"), &argv).is_ok());
    }

    #[test]
    fn empty_argv_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let r = write_script(dir.path(), "x", Path::new("/"), &[]);
        assert!(matches!(r, Err(ScriptError::EmptyArgv)));
        assert!(!dir.path().join("x.command").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_at_target_is_replaced_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let link = dir.path().join("x.command");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        let path = write_script(dir.path(), "x", Path::new("/"), &strings(&["true"])).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_file()
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .ends_with("exec 'true'\n")
        );
    }

    #[test]
    fn open_argv_appends_script() {
        let argv = open_argv(
            &strings(&["open", "-a", "Terminal"]),
            Path::new("/d/x.command"),
        );
        assert_eq!(argv, strings(&["open", "-a", "Terminal", "/d/x.command"]));
    }

    #[test]
    fn editor_argv_splits_words() {
        let f = Path::new("/d/plan.toml");
        assert_eq!(
            editor_argv(Some("code --wait"), Some("nano"), f),
            strings(&["code", "--wait", "/d/plan.toml"])
        );
        assert_eq!(
            editor_argv(None, Some(" emacs  -nw "), f),
            strings(&["emacs", "-nw", "/d/plan.toml"])
        );
        assert_eq!(
            editor_argv(Some("  "), None, f),
            strings(&["vi", "/d/plan.toml"])
        );
        assert_eq!(editor_argv(None, None, f), strings(&["vi", "/d/plan.toml"]));
    }
}
