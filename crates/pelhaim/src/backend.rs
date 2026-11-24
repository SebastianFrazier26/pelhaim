use std::path::PathBuf;

/// F38: the board emits `Reveal(cwd)` raw; only an existing directory is revealed, by its
/// canonical path, so a stale or crafted cwd never reaches `open -R` / `xdg-open`.
pub fn reveal_target(cwd: &str) -> Option<PathBuf> {
    let p = std::fs::canonicalize(cwd).ok()?;
    p.is_dir().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_only_existing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(reveal_target(file.to_str().unwrap()), None);
        assert_eq!(
            reveal_target(dir.path().join("missing").to_str().unwrap()),
            None
        );
        let got = reveal_target(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(got, std::fs::canonicalize(dir.path()).unwrap());
        assert!(got.is_absolute());
    }
}
