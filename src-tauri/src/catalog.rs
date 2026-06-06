//! Culling catalog: ratings, color labels, and pick/reject flags.
//!
//! Keyed by **path relative to the library root** (not absolute), so the same
//! SQLite file stays valid when the SSD is mounted under a different drive
//! letter / mount point on another machine (Alienware `E:\` vs Mac
//! `/Volumes/...`). Nothing here is written next to the user's originals.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;

pub struct Catalog {
    conn: Mutex<Connection>,
}

#[derive(Serialize, Clone, Default)]
pub struct Decision {
    pub rel: String,
    pub rating: i64,
    pub label: Option<String>,
    pub flag: Option<String>, // "pick" | "reject" | None
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Catalog {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS decisions (
                rel        TEXT PRIMARY KEY,
                rating     INTEGER NOT NULL DEFAULT 0,
                label      TEXT,
                flag       TEXT,
                updated_at INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_decisions_flag ON decisions(flag)",
            [],
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Fetch every decision at or under a rel-path prefix in a SINGLE query.
    /// `prefix` is the selected folder relative to the library root ("" = root).
    /// This replaces the old per-file loop, which did thousands of queries for a
    /// big folder. Matches the folder itself and everything beneath it.
    pub fn get_under(&self, prefix: &str) -> Vec<Decision> {
        let conn = self.conn.lock();
        let (sql, like): (&str, String) = if prefix.is_empty() {
            ("SELECT rel, rating, label, flag FROM decisions", String::new())
        } else {
            (
                "SELECT rel, rating, label, flag FROM decisions WHERE rel = ?1 OR rel LIKE ?2",
                format!("{prefix}/%"),
            )
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let map = |r: &rusqlite::Row<'_>| {
            Ok(Decision {
                rel: r.get(0)?,
                rating: r.get(1)?,
                label: r.get(2)?,
                flag: r.get(3)?,
            })
        };
        let rows = if prefix.is_empty() {
            stmt.query_map([], map)
        } else {
            stmt.query_map(params![prefix, like], map)
        };
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn set_rating(&self, rel: &str, rating: i64) -> rusqlite::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO decisions(rel, rating, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(rel) DO UPDATE SET rating = ?2, updated_at = ?3",
            params![rel, rating, now()],
        )?;
        Ok(())
    }

    pub fn set_label(&self, rel: &str, label: Option<String>) -> rusqlite::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO decisions(rel, label, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(rel) DO UPDATE SET label = ?2, updated_at = ?3",
            params![rel, label, now()],
        )?;
        Ok(())
    }

    pub fn set_flag(&self, rel: &str, flag: Option<String>) -> rusqlite::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO decisions(rel, flag, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(rel) DO UPDATE SET flag = ?2, updated_at = ?3",
            params![rel, flag, now()],
        )?;
        Ok(())
    }

    /// Batch upsert one column for many rel-paths in a single transaction —
    /// avoids firing thousands of IPC calls / writes on a "select all → reject".
    fn set_col_many(
        &self,
        col: &str,
        rels: &[String],
        val: rusqlite::types::Value,
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let sql = format!(
                "INSERT INTO decisions(rel, {col}, updated_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(rel) DO UPDATE SET {col} = ?2, updated_at = ?3"
            );
            let mut stmt = tx.prepare(&sql)?;
            let t = now();
            for rel in rels {
                stmt.execute(params![rel, val, t])?;
            }
        }
        tx.commit()
    }

    pub fn set_rating_many(&self, rels: &[String], rating: i64) -> rusqlite::Result<()> {
        self.set_col_many("rating", rels, rating.into())
    }

    pub fn set_label_many(&self, rels: &[String], label: Option<String>) -> rusqlite::Result<()> {
        self.set_col_many("label", rels, label.into())
    }

    pub fn set_flag_many(&self, rels: &[String], flag: Option<String>) -> rusqlite::Result<()> {
        self.set_col_many("flag", rels, flag.into())
    }

    /// All rel-paths currently carrying a given flag (e.g. "reject"), across the
    /// whole catalog. Used to build the delete-sweep list.
    pub fn list_by_flag(&self, flag: &str) -> Vec<String> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare("SELECT rel FROM decisions WHERE flag = ?1") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![flag], |r| r.get::<_, String>(0));
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Drop catalog rows for rel-paths that no longer exist (e.g. after a
    /// successful trash sweep). Best-effort; ignores individual failures.
    pub fn forget(&self, rels: &[String]) {
        let conn = self.conn.lock();
        for rel in rels {
            let _ = conn.execute("DELETE FROM decisions WHERE rel = ?1", params![rel]);
        }
    }
}
