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

/// `<dir>/<name>.command`: macOS opens that extension in Terminal.
pub fn write_script(
    dir: &Path,
    name: &str,
    cwd: &Path,
    argv: &[String],
) -> std::io::Result<PathBuf> {
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

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o700);
    }
    let mut f = opts.open(&path)?;
    // `mode` applies only when the file is created; an older script keeps its bits otherwise.
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
