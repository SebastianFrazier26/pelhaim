use std::collections::HashMap;
use std::path::PathBuf;

pub trait Env {
    fn var(&self, k: &str) -> Option<String>;
    fn home(&self) -> PathBuf;
}

pub struct ProcessEnv;

impl Env for ProcessEnv {
    fn var(&self, k: &str) -> Option<String> {
        std::env::var(k).ok().filter(|v| !v.is_empty())
    }

    fn home(&self) -> PathBuf {
        std::env::home_dir().unwrap_or_default()
    }
}

pub struct MapEnv {
    vars: HashMap<String, String>,
    home: PathBuf,
}

impl MapEnv {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            vars: HashMap::new(),
            home: home.into(),
        }
    }

    pub fn with(mut self, k: &str, v: &str) -> Self {
        self.vars.insert(k.to_string(), v.to_string());
        self
    }
}

impl Env for MapEnv {
    fn var(&self, k: &str) -> Option<String> {
        self.vars.get(k).filter(|v| !v.is_empty()).cloned()
    }

    fn home(&self) -> PathBuf {
        self.home.clone()
    }
}

/// Expands `~` and `~/…` only; `~user` is left as written.
pub fn expand_tilde(p: &str, env: &dyn Env) -> PathBuf {
    if p == "~" {
        env.home()
    } else if let Some(rest) = p.strip_prefix("~/") {
        env.home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_cases() {
        let env = MapEnv::new("/home/u");
        assert_eq!(expand_tilde("~", &env), PathBuf::from("/home/u"));
        assert_eq!(expand_tilde("~/a/b", &env), PathBuf::from("/home/u/a/b"));
        assert_eq!(expand_tilde("/abs", &env), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("~bob/x", &env), PathBuf::from("~bob/x"));
    }

    #[test]
    fn empty_var_counts_as_unset() {
        let env = MapEnv::new("/h").with("X", "");
        assert_eq!(env.var("X"), None);
    }
}
