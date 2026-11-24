use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use pelhaim_core::{AlertId, AssignTarget, Decision, OwnerEvent, RestoredState, SessionKey, Ts};
use rusqlite::{Connection, Transaction, params};

const SCHEMA_VERSION: i64 = 1;

// The unique index keeps decision logging idempotent across restarts: the
// engine forgets which alerts it logged, and may log one again under a new id.
const SCHEMA_V1: &str = "
CREATE TABLE events(id INTEGER PRIMARY KEY, ts TEXT NOT NULL, source TEXT NOT NULL, kind TEXT NOT NULL, payload_json TEXT NOT NULL);
CREATE TABLE assignments(session_key TEXT PRIMARY KEY, target_json TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE tray(alert_id TEXT PRIMARY KEY);
CREATE TABLE decisions(id INTEGER PRIMARY KEY, json TEXT NOT NULL, written_to_md INTEGER NOT NULL);
CREATE UNIQUE INDEX decisions_source_alert ON decisions(json_extract(json, '$.source_alert'));
PRAGMA user_version = 1;
";

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Timestamp(chrono::ParseError),
    NewerSchema(i64),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "store directory: {e}"),
            StoreError::Sqlite(e) => write!(f, "store database: {e}"),
            StoreError::Json(e) => write!(f, "store record: {e}"),
            StoreError::Timestamp(e) => write!(f, "store timestamp: {e}"),
            StoreError::NewerSchema(v) => write!(
                f,
                "the store has schema version {v}, newer than this pelhaim ({SCHEMA_VERSION})"
            ),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(e) => Some(e),
            StoreError::Sqlite(e) => Some(e),
            StoreError::Json(e) => Some(e),
            StoreError::Timestamp(e) => Some(e),
            StoreError::NewerSchema(_) => None,
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}

impl From<chrono::ParseError> for StoreError {
    fn from(e: chrono::ParseError) -> Self {
        StoreError::Timestamp(e)
    }
}

/// Owner events plus their projections. Observe-derived state is never
/// stored; it is rebuilt from the session files on disk.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        migrate(&mut conn)?;
        Ok(Store { conn })
    }

    pub fn append(&mut self, ev: &OwnerEvent, at: Ts) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO events(ts, source, kind, payload_json) VALUES (?1, 'owner', ?2, ?3)",
            params![ts_text(at), kind(ev), serde_json::to_string(ev)?],
        )?;
        project(&tx, ev, at)?;
        tx.commit()?;
        Ok(())
    }

    pub fn load(&self) -> Result<RestoredState, StoreError> {
        let mut assignments = Vec::new();
        let mut stmt = self
            .conn
            .prepare("SELECT session_key, target_json, at FROM assignments ORDER BY session_key")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let target: AssignTarget = serde_json::from_str(&r.get::<_, String>(1)?)?;
            assignments.push((
                SessionKey(r.get(0)?),
                target,
                parse_ts(&r.get::<_, String>(2)?)?,
            ));
        }

        let mut stmt = self.conn.prepare("SELECT alert_id FROM tray")?;
        let tray = stmt
            .query_map([], |r| r.get(0).map(AlertId))?
            .collect::<Result<_, _>>()?;

        let decisions = self.query_decisions(
            "SELECT json, written_to_md FROM decisions ORDER BY id LIMIT ?1",
            -1,
        )?;
        Ok(RestoredState {
            assignments,
            tray,
            decisions,
        })
    }

    /// Newest first.
    pub fn decisions(&self, limit: usize) -> Result<Vec<Decision>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.query_decisions(
            "SELECT json, written_to_md FROM decisions ORDER BY id DESC LIMIT ?1",
            limit,
        )
    }

    pub fn rebuild_projections(&mut self) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("DELETE FROM assignments; DELETE FROM tray; DELETE FROM decisions;")?;
        let events = {
            let mut stmt = tx.prepare("SELECT ts, payload_json FROM events ORDER BY id")?;
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (ts, payload) in events {
            let ev: OwnerEvent = serde_json::from_str(&payload)?;
            project(&tx, &ev, parse_ts(&ts)?)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn query_decisions(&self, sql: &str, limit: i64) -> Result<Vec<Decision>, StoreError> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let mut d: Decision = serde_json::from_str(&r.get::<_, String>(0)?)?;
            d.written_to_md = r.get::<_, i64>(1)? != 0;
            out.push(d);
        }
        Ok(out)
    }
}

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema(version));
    }
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.commit()?;
    }
    Ok(())
}

fn project(tx: &Transaction<'_>, ev: &OwnerEvent, at: Ts) -> Result<(), StoreError> {
    match ev {
        OwnerEvent::Assigned { session, target } => {
            tx.execute(
                "INSERT INTO assignments(session_key, target_json, at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(session_key) DO UPDATE SET target_json = excluded.target_json, at = excluded.at",
                params![session.0, serde_json::to_string(target)?, ts_text(at)],
            )?;
        }
        OwnerEvent::Dismissed { alert } => {
            tx.execute(
                "INSERT OR IGNORE INTO tray(alert_id) VALUES (?1)",
                params![alert.0],
            )?;
        }
        OwnerEvent::Reopened { alert } => {
            tx.execute("DELETE FROM tray WHERE alert_id = ?1", params![alert.0])?;
        }
        // Answers have no projection: alerts are observe-derived, and an
        // answer that assigns a session arrives as its own `Assigned` event.
        OwnerEvent::Answered { .. } => {}
        OwnerEvent::DecisionLogged(d) => {
            tx.execute(
                "INSERT OR IGNORE INTO decisions(id, json, written_to_md) VALUES (?1, ?2, ?3)",
                params![d.id.0, serde_json::to_string(d)?, d.written_to_md],
            )?;
        }
        OwnerEvent::DecisionExported { id } => {
            tx.execute(
                "UPDATE decisions SET written_to_md = 1 WHERE id = ?1",
                params![id.0],
            )?;
        }
    }
    Ok(())
}

fn kind(ev: &OwnerEvent) -> &'static str {
    match ev {
        OwnerEvent::Assigned { .. } => "Assigned",
        OwnerEvent::Dismissed { .. } => "Dismissed",
        OwnerEvent::Reopened { .. } => "Reopened",
        OwnerEvent::Answered { .. } => "Answered",
        OwnerEvent::DecisionLogged(_) => "DecisionLogged",
        OwnerEvent::DecisionExported { .. } => "DecisionExported",
    }
}

fn ts_text(at: Ts) -> String {
    at.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn parse_ts(s: &str) -> Result<Ts, StoreError> {
    Ok(DateTime::parse_from_rfc3339(s)?.with_timezone(&Utc))
}
