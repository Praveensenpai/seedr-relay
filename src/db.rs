//! `SQLite` persistence for the magnet queue.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Status of an item in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStatus {
    Pending,
    Caching,
    Downloading,
    Completed,
    Failed,
}

impl QueueStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Caching => "caching",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parses status string from database.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "caching" => Self::Caching,
            "downloading" => Self::Downloading,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

impl fmt::Display for QueueStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An entry in the download queue.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub id: i64,
    pub magnet: String,
    pub name: String,
    pub status: QueueStatus,
    pub output_dir: Option<PathBuf>,
    pub created_at: String,
    pub error_msg: Option<String>,
}

/// Database handle for queue operations.
pub struct QueueDb {
    conn: Connection,
}

impl QueueDb {
    /// Opens the queue database at the standard data path.
    ///
    /// # Errors
    /// Returns an error if the directory or database cannot be opened.
    pub fn open() -> Result<Self> {
        let db_path = data_dir().join("queue.db");
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create data dir {}", parent.display()))?;
        }
        let conn = Connection::open(&db_path)
            .with_context(|| format!("failed to open database at {}", db_path.display()))?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    /// Initializes tables and indexes.
    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                magnet TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                output_dir TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error_msg TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_queue_status ON queue(status);",
        ).context("failed to execute queue database initialization")?;
        Ok(())
    }

    /// Inserts a new magnet link into the queue.
    ///
    /// # Errors
    /// Returns an error if insertion fails.
    pub fn enqueue(&self, magnet: &str, output_dir: Option<&Path>) -> Result<QueueItem> {
        let now = Utc::now().to_rfc3339();
        let name_hint = seedr_dl::extract_magnet_name(magnet)
            .unwrap_or_else(|| "unnamed-torrent".to_string());
        let out_str = output_dir.map(|p| p.to_string_lossy().to_string());

        self.conn.execute(
            "INSERT INTO queue (magnet, name, status, output_dir, created_at, updated_at, error_msg)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)",
            params![magnet, name_hint, QueueStatus::Pending.as_str(), out_str, now],
        ).context("failed to insert item into queue")?;

        let id = self.conn.last_insert_rowid();
        Ok(QueueItem {
            id,
            magnet: magnet.to_string(),
            name: name_hint,
            status: QueueStatus::Pending,
            output_dir: output_dir.map(Path::to_path_buf),
            created_at: now,
            error_msg: None,
        })
    }

    /// Returns the next pending queue item in FIFO order.
    ///
    /// # Errors
    /// Returns an error if querying fails.
    pub fn next_pending(&self) -> Result<Option<QueueItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, magnet, name, status, output_dir, created_at, error_msg
             FROM queue WHERE status = 'pending' ORDER BY id ASC LIMIT 1",
        ).context("failed to prepare next_pending query")?;

        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let out_str: Option<String> = row.get(4)?;
            let status_str: String = row.get(3)?;
            Ok(Some(QueueItem {
                id: row.get(0)?,
                magnet: row.get(1)?,
                name: row.get(2)?,
                status: QueueStatus::parse(&status_str),
                output_dir: out_str.map(PathBuf::from),
                created_at: row.get(5)?,
                error_msg: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Updates status and optional error message.
    ///
    /// # Errors
    /// Returns an error if the update query fails.
    pub fn update_status(&self, id: i64, status: QueueStatus, error: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE queue SET status = ?1, error_msg = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), error, now, id],
        ).context("failed to update queue item status")?;
        Ok(())
    }

    /// Updates the resolved item name.
    ///
    /// # Errors
    /// Returns an error if the update query fails.
    pub fn update_name(&self, id: i64, name: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE queue SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, id],
        ).context("failed to update item name")?;
        Ok(())
    }

    /// Lists recent queue items.
    ///
    /// # Errors
    /// Returns an error if querying fails.
    pub fn list_items(&self, limit: usize) -> Result<Vec<QueueItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, magnet, name, status, output_dir, created_at, error_msg
             FROM queue ORDER BY id DESC LIMIT ?1",
        ).context("failed to prepare list_items query")?;

        let rows = stmt.query_map([limit], |row| {
            let out_str: Option<String> = row.get(4)?;
            let status_str: String = row.get(3)?;
            Ok(QueueItem {
                id: row.get(0)?,
                magnet: row.get(1)?,
                name: row.get(2)?,
                status: QueueStatus::parse(&status_str),
                output_dir: out_str.map(PathBuf::from),
                created_at: row.get(5)?,
                error_msg: row.get(6)?,
            })
        })?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok(items)
    }

    /// Removes an item from the queue by ID.
    ///
    /// # Errors
    /// Returns an error if deletion fails.
    pub fn remove_item(&self, id: i64) -> Result<bool> {
        let affected = self.conn.execute("DELETE FROM queue WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Cleans all completed items from the queue.
    ///
    /// # Errors
    /// Returns an error if deletion fails.
    pub fn clean_completed(&self) -> Result<usize> {
        let count = self.conn.execute("DELETE FROM queue WHERE status = 'completed'", [])?;
        Ok(count)
    }
}

/// Returns data directory `~/.local/share/seedr-relay`.
#[must_use]
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("seedr-relay")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_lifecycle_works() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        let db = QueueDb { conn };
        db.init()?;

        let item = db.enqueue("magnet:?xt=urn:btih:test&dn=Sample.Movie.2024", None)?;
        assert_eq!(item.name, "Sample.Movie.2024");
        assert_eq!(item.status, QueueStatus::Pending);

        let next = db.next_pending()?;
        let next_item = next.ok_or("expected pending item")?;
        assert_eq!(next_item.id, item.id);

        db.update_status(item.id, QueueStatus::Completed, None)?;
        let after_complete = db.next_pending()?;
        assert!(after_complete.is_none());
        Ok(())
    }
}
