use serde::{Deserialize, Serialize};

/// `claude:<sid>`, `claude:<sid>/<agent_id>` or `codex:<id>`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct SessionKey(pub String);

impl SessionKey {
    pub fn claude(sid: &str) -> Self {
        Self(format!("claude:{sid}"))
    }

    pub fn claude_sub(sid: &str, agent_id: &str) -> Self {
        Self(format!("claude:{sid}/{agent_id}"))
    }

    pub fn codex(id: &str) -> Self {
        Self(format!("codex:{id}"))
    }

    /// The raw tool session id, without the tool prefix or `/<agent_id>`.
    pub fn session_id(&self) -> &str {
        let rest = self.0.split_once(':').map_or(self.0.as_str(), |(_, r)| r);
        rest.split_once('/').map_or(rest, |(sid, _)| sid)
    }
}

macro_rules! numeric_id {
    ($($name:ident($t:ty)),* $(,)?) => {$(
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
        #[serde(transparent)]
        pub struct $name(pub $t);
    )*};
}

numeric_id!(
    TrainId(u32),
    TrackId(u32),
    StationId(u32),
    ForkId(u32),
    DecisionId(i64),
    PlanId(u32),
);

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct AlertId(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_ids() {
        assert_eq!(SessionKey::claude_sub("S1", "a9").session_id(), "S1");
    }

    #[test]
    fn session_key_formats() {
        assert_eq!(SessionKey::claude("S1").0, "claude:S1");
        assert_eq!(SessionKey::claude_sub("S1", "a9").0, "claude:S1/a9");
        assert_eq!(SessionKey::codex("C7").0, "codex:C7");
        assert_eq!(SessionKey::claude("S1").session_id(), "S1");
        assert_eq!(SessionKey::codex("C7").session_id(), "C7");
    }

    #[test]
    fn ids_serialize_transparently() {
        assert_eq!(
            serde_json::to_string(&SessionKey::codex("C7")).unwrap(),
            "\"codex:C7\""
        );
        assert_eq!(serde_json::to_string(&TrainId(3)).unwrap(), "3");
        let back: DecisionId = serde_json::from_str("-4").unwrap();
        assert_eq!(back, DecisionId(-4));
    }
}
