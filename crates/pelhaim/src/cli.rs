use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The M1 subset of spec §6.7: `plan` and `launch` arrive with Launch mode (M2).
#[derive(Debug, Parser)]
#[command(name = "pelhaim", version, about)]
pub struct Cli {
    /// Config file (default: $PELHAIM_CONFIG, then ~/.config/pelhaim/config.toml).
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Run without a window; needs --dump-scene.
    #[arg(long, requires = "dump_scene")]
    pub headless: bool,
    /// Catch up once, write the board's display list as JSON to PATH and exit.
    #[arg(long, value_name = "PATH", requires = "headless")]
    pub dump_scene: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Cmd {
    /// Open the board in observe mode (the only mode in M1).
    Observe,
    /// Check CLI versions, config, data dir, decision log, keep-awake and GPU.
    Doctor,
    /// Print logged decisions, newest first.
    Decisions {
        /// Print the open decisions from DECISION-LOG.md instead.
        #[arg(long)]
        open: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_m1_surface() {
        let c = Cli::try_parse_from(["pelhaim"]).unwrap();
        assert_eq!(c.command, None);
        let c = Cli::try_parse_from(["pelhaim", "decisions", "--open"]).unwrap();
        assert_eq!(c.command, Some(Cmd::Decisions { open: true }));
        let c = Cli::try_parse_from(["pelhaim", "--headless", "--dump-scene", "o.json", "observe"])
            .unwrap();
        assert!(c.headless);
        assert_eq!(c.dump_scene, Some(PathBuf::from("o.json")));
        assert!(Cli::try_parse_from(["pelhaim", "--dump-scene", "o.json"]).is_err());
        assert!(Cli::try_parse_from(["pelhaim", "--headless"]).is_err());
        assert!(Cli::try_parse_from(["pelhaim", "launch"]).is_err());
    }
}
