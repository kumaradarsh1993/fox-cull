//! Culling catalog: ratings, color labels, and pick/reject flags.
//!
//! Keyed by **path relative to the library root** (not absolute), so the same
//! SQLite file stays valid when the SSD is mounted under a different drive
//! letter / mount point on another machine (Alienware `E:\` vs Mac
//! `/Volumes/...`). Nothing here is written next to the user's originals.

use std::collections::HashMap;
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

#[derive(Serialize, Clone)]
pub struct TrashRow {
    pub stored: String,
    pub orig: String,
    pub name: String,
    pub deleted_at: i64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Catalog {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Ok(Self {
            conn: Mutex::new(Self::open_conn(path)?),
        })
    }

    /// Re-point the catalog at a different file (the user moved it onto their
    /// SSD, or back to the default). The in-flight connection is swapped under
    /// the lock; callers migrate/copy the file first if needed.
    pub fn reopen(&self, path: &Path) -> rusqlite::Result<()> {
        let conn = Self::open_conn(path)?;
        *self.conn.lock() = conn;
        Ok(())
    }

    /// Fold the WAL back into the main file so a plain file-copy relocation is
    /// complete (no orphaned `-wal`/`-shm`).
    pub fn checkpoint(&self) {
        let conn = self.conn.lock();
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    fn open_conn(path: &Path) -> rusqlite::Result<Connection> {
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
        // Free-form per-file tags (many-to-many), keyed by the same rel-path so
        // the catalog stays portable. Cull-relevant for "Diwali across S23U/2026".
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tags (
                rel TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (rel, tag)
            )",
            [],
        )?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag)", [])?;
        // Per-video trim in/out points (seconds), for the lossless cut feature.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trims (
                rel   TEXT PRIMARY KEY,
                in_s  REAL NOT NULL,
                out_s REAL NOT NULL
            )",
            [],
        )?;
        // Cached capture timestamps (EXIF DateTimeOriginal / video creation_time),
        // keyed by rel-path and validated against (mtime, size) so a replaced file
        // re-indexes. Lets folder-open stay instant (no eager EXIF) while sorting
        // and month-grouping use the real capture date, filled in the background.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS captures (
                rel      TEXT PRIMARY KEY,
                captured INTEGER NOT NULL,
                mtime    INTEGER NOT NULL,
                size     INTEGER NOT NULL
            )",
            [],
        )?;
        // In-app Trash: files moved into the per-drive recycle folder by a
        // folder-mode delete. `stored` = path within the recycle dir; `orig` =
        // original path relative to the drive root (for Restore); `deleted_at`
        // drives the "most recently rejected first" sort. Permanent purge or a
        // successful restore removes the row.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trash (
                stored     TEXT PRIMARY KEY,
                orig       TEXT NOT NULL,
                name       TEXT NOT NULL,
                deleted_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(conn)
    }

    /// Stored trim in/out (seconds) for a clip, if any.
    pub fn get_trim(&self, rel: &str) -> Option<(f64, f64)> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT in_s, out_s FROM trims WHERE rel = ?1",
            params![rel],
            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?)),
        )
        .ok()
    }

    pub fn set_trim(&self, rel: &str, in_s: f64, out_s: f64) -> rusqlite::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO trims(rel, in_s, out_s) VALUES(?1, ?2, ?3)
             ON CONFLICT(rel) DO UPDATE SET in_s = ?2, out_s = ?3",
            params![rel, in_s, out_s],
        )?;
        Ok(())
    }

    pub fn clear_trim(&self, rel: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute("DELETE FROM trims WHERE rel = ?1", params![rel]);
    }

    // ── capture-date cache ───────────────────────────────────────────────────

    /// Cached captures at or under a rel-prefix: rel → (captured, mtime, size).
    /// One query for the whole subtree (mirrors `get_under`); the caller checks
    /// (mtime, size) to decide whether a row is still valid.
    pub fn captures_under(&self, prefix: &str) -> HashMap<String, (i64, i64, i64)> {
        let conn = self.conn.lock();
        let (sql, like): (&str, String) = if prefix.is_empty() {
            (
                "SELECT rel, captured, mtime, size FROM captures",
                String::new(),
            )
        } else {
            (
                "SELECT rel, captured, mtime, size FROM captures WHERE rel = ?1 OR rel LIKE ?2",
                format!("{prefix}/%"),
            )
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let map = |r: &rusqlite::Row<'_>| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?),
            ))
        };
        let rows = if prefix.is_empty() {
            stmt.query_map([], map)
        } else {
            stmt.query_map(params![prefix, like], map)
        };
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => HashMap::new(),
        }
    }

    /// Upsert many capture rows (rel, captured, mtime, size) in one transaction.
    pub fn set_capture_many(&self, rows: &[(String, i64, i64, i64)]) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO captures(rel, captured, mtime, size) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(rel) DO UPDATE SET captured = ?2, mtime = ?3, size = ?4",
            )?;
            for (rel, captured, mtime, size) in rows {
                stmt.execute(params![rel, captured, mtime, size])?;
            }
        }
        tx.commit()
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
            let _ = conn.execute("DELETE FROM tags WHERE rel = ?1", params![rel]);
            let _ = conn.execute("DELETE FROM trims WHERE rel = ?1", params![rel]);
            let _ = conn.execute("DELETE FROM captures WHERE rel = ?1", params![rel]);
        }
    }

    // ── tags ────────────────────────────────────────────────────────────────

    /// Every tag at or under a rel-prefix, grouped by rel-path, in one query.
    pub fn tags_under(&self, prefix: &str) -> HashMap<String, Vec<String>> {
        let conn = self.conn.lock();
        let (sql, bind): (&str, Option<String>) = if prefix.is_empty() {
            ("SELECT rel, tag FROM tags ORDER BY tag", None)
        } else {
            (
                "SELECT rel, tag FROM tags WHERE rel = ?1 OR rel LIKE ?2 ORDER BY tag",
                Some(format!("{prefix}/%")),
            )
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let map = |r: &rusqlite::Row<'_>| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?));
        let rows = match &bind {
            None => stmt.query_map([], map),
            Some(like) => stmt.query_map(params![prefix, like], map),
        };
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(it) = rows {
            for (rel, tag) in it.flatten() {
                out.entry(rel).or_default().push(tag);
            }
        }
        out
    }

    /// Distinct tag names across the whole catalog (for the filter UI), with usage
    /// counts, most-used first.
    pub fn all_tags(&self) -> Vec<(String, i64)> {
        let conn = self.conn.lock();
        let mut stmt = match conn
            .prepare("SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY COUNT(*) DESC, tag")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)));
        match rows {
            Ok(it) => it.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Add `tag` to many rel-paths in one transaction (idempotent).
    pub fn add_tag_many(&self, rels: &[String], tag: &str) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR IGNORE INTO tags(rel, tag) VALUES(?1, ?2)")?;
            for rel in rels {
                stmt.execute(params![rel, tag])?;
            }
        }
        tx.commit()
    }

    /// Remove `tag` from many rel-paths in one transaction.
    pub fn remove_tag_many(&self, rels: &[String], tag: &str) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM tags WHERE rel = ?1 AND tag = ?2")?;
            for rel in rels {
                stmt.execute(params![rel, tag])?;
            }
        }
        tx.commit()
    }

    // ── trash (in-app, per-drive recycle) ─────────────────────────────────────

    /// Record many disposed files in one transaction: (stored, orig, name, at).
    pub fn add_trash_many(
        &self,
        rows: &[(String, String, String, i64)],
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO trash(stored, orig, name, deleted_at) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(stored) DO UPDATE SET orig = ?2, name = ?3, deleted_at = ?4",
            )?;
            for (stored, orig, name, at) in rows {
                stmt.execute(params![stored, orig, name, at])?;
            }
        }
        tx.commit()
    }

    /// All trashed files, most recently rejected first.
    pub fn list_trash(&self) -> Vec<TrashRow> {
        let conn = self.conn.lock();
        let mut stmt = match conn
            .prepare("SELECT stored, orig, name, deleted_at FROM trash ORDER BY deleted_at DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |r| {
            Ok(TrashRow {
                stored: r.get(0)?,
                orig: r.get(1)?,
                name: r.get(2)?,
                deleted_at: r.get(3)?,
            })
        });
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Forget trash rows (after a successful restore or permanent purge).
    pub fn remove_trash(&self, stored: &[String]) {
        let conn = self.conn.lock();
        for s in stored {
            let _ = conn.execute("DELETE FROM trash WHERE stored = ?1", params![s]);
        }
    }
}
