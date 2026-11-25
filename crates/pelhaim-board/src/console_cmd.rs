#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConsoleCmd {
    Goto(String),
    Decisions,
    DecisionsExport,
    Mode,
    Help,
    Train { number: String, text: String },
    Unknown(String),
    Empty,
}

pub fn parse_console(s: &str) -> ConsoleCmd {
    let s = s.trim();
    if s.is_empty() {
        return ConsoleCmd::Empty;
    }
    if let Some(rest) = s.strip_prefix('@') {
        let (number, text) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if !number.is_empty() {
            return ConsoleCmd::Train {
                number: number.into(),
                text: text.trim().into(),
            };
        }
        return ConsoleCmd::Unknown(s.into());
    }
    let mut words = s.split_whitespace();
    let cmd = words.next().unwrap_or_default();
    let args: Vec<&str> = words.collect();
    match (cmd, args.as_slice()) {
        ("/goto", [_, ..]) => ConsoleCmd::Goto(args.join(" ")),
        ("/decisions", []) => ConsoleCmd::Decisions,
        ("/decisions", ["export"]) => ConsoleCmd::DecisionsExport,
        ("/mode", []) => ConsoleCmd::Mode,
        ("/help", []) => ConsoleCmd::Help,
        _ => ConsoleCmd::Unknown(s.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_commands() {
        assert_eq!(
            parse_console("/goto ethica"),
            ConsoleCmd::Goto("ethica".into())
        );
        assert!(matches!(
            parse_console("@1148 hi"),
            ConsoleCmd::Train { .. }
        ));
        assert_eq!(
            parse_console("  @1148   hi there "),
            ConsoleCmd::Train {
                number: "1148".into(),
                text: "hi there".into()
            }
        );
        assert_eq!(parse_console("/decisions"), ConsoleCmd::Decisions);
        assert_eq!(
            parse_console("/decisions export"),
            ConsoleCmd::DecisionsExport
        );
        assert_eq!(parse_console("/mode"), ConsoleCmd::Mode);
        assert_eq!(parse_console("/help"), ConsoleCmd::Help);
        assert_eq!(parse_console("   "), ConsoleCmd::Empty);
    }

    #[test]
    fn malformed_commands_are_unknown() {
        for s in [
            "/goto",
            "/mode now",
            "/decisions purge",
            "@",
            "hello",
            "/nope",
        ] {
            assert_eq!(parse_console(s), ConsoleCmd::Unknown(s.into()), "{s}");
        }
    }
}
