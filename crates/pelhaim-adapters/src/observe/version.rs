pub const CLAUDE_TESTED: &[&str] = &["2.1.331"];
pub const CODEX_TESTED: &[&str] = &["0.158.0"];

fn parse(version: &str) -> Option<Vec<u64>> {
    let mut parts: Vec<u64> = version
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    // Trailing zeros are dropped so "2.1" and "2.1.0" compare equal.
    while parts.last() == Some(&0) {
        parts.pop();
    }
    Some(parts)
}

/// True when `version` is newer than every tested version, or unparseable.
pub fn is_untested(version: &str, tested: &[&str]) -> bool {
    let Some(v) = parse(version) else {
        return true;
    };
    match tested.iter().filter_map(|t| parse(t)).max() {
        Some(max) => v > max,
        None => true,
    }
}
