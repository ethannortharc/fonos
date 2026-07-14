//! Onboarding funnel — record-once first-experience milestones, local-only.
//!
//! Six steps (launch / mic_granted / first_transcript / ax_granted /
//! first_insert / first_command) are each written at most once via a UNIQUE
//! constraint + INSERT OR IGNORE. Data never leaves the device.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// One recorded funnel milestone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunnelEvent {
    /// Row ID.
    pub id: i64,
    /// Milestone name (e.g. "first_transcript"). Unique — recorded once.
    pub step: String,
    /// ISO 8601 timestamp of the first time the milestone was reached.
    pub created_at: String,
}

/// Create the onboarding_events table (idempotent).
pub fn init_db(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS onboarding_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            step TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
        );",
    )
    .expect("funnel: init_db failed");
}

/// Record a milestone once. Returns true when this call recorded it,
/// false when it was already present (INSERT OR IGNORE hit the UNIQUE row).
pub fn record(conn: &Connection, step: &str) -> Result<bool> {
    let n = conn
        .execute(
            "INSERT OR IGNORE INTO onboarding_events (step) VALUES (?1)",
            params![step],
        )
        .map_err(|e| Error::Database(format!("funnel record: {e}")))?;
    Ok(n > 0)
}

/// All recorded milestones, oldest first (id breaks same-second ties).
pub fn get_all(conn: &Connection) -> Result<Vec<FunnelEvent>> {
    let mut stmt = conn
        .prepare("SELECT id, step, created_at FROM onboarding_events ORDER BY created_at ASC, id ASC")
        .map_err(|e| Error::Database(format!("funnel get_all prepare: {e}")))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FunnelEvent {
                id: r.get(0)?,
                step: r.get(1)?,
                created_at: r.get(2)?,
            })
        })
        .map_err(|e| Error::Database(format!("funnel get_all query: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| Error::Database(format!("funnel get_all row: {e}")))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_is_once_and_get_all_orders() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_db(&conn);

        assert!(record(&conn, "launch").expect("first record"));
        assert!(!record(&conn, "launch").expect("repeat is ignored"));
        assert!(record(&conn, "mic_granted").expect("second step"));

        let all = get_all(&conn).expect("get_all");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].step, "launch");
        assert_eq!(all[1].step, "mic_granted");
        assert!(!all[0].created_at.is_empty());
    }

    #[test]
    fn init_db_is_idempotent() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_db(&conn);
        init_db(&conn);
        assert!(record(&conn, "launch").expect("record after double init"));
    }
}
