use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
    SqliteSynchronous,
};

use super::{Envelope, EnvelopeRef, Storage, StorageError, StorageResult};
use crate::model::{Comment, Paste};

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        match e.as_database_error() {
            Some(db) if db.is_unique_violation() => Self::AlreadyExists,
            _ => Self::Backend(e.to_string()),
        }
    }
}

impl From<sqlx::migrate::MigrateError> for StorageError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Backend(format!("migration failed: {e}"))
    }
}

/// SQLite backend (WAL mode). Schema lives in `migrations/`.
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Opens (creating if needed) the database file and runs migrations.
    pub async fn open(path: &Path) -> StorageResult<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent).await?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        Self::connect(options, 8).await
    }

    /// Private in-memory database, mainly for tests.
    pub async fn in_memory() -> StorageResult<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // Every connection to `:memory:` is a distinct database: use exactly one.
        Self::connect(options, 1).await
    }

    async fn connect(options: SqliteConnectOptions, max: u32) -> StorageResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .min_connections(if max == 1 { 1 } else { 0 })
            .idle_timeout(if max == 1 {
                None
            } else {
                Some(Duration::from_secs(600))
            })
            .max_lifetime(if max == 1 {
                None
            } else {
                Some(Duration::from_secs(3600))
            })
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

fn paste_from_row(id: &str, row: &SqliteRow) -> StorageResult<Paste> {
    let data: Vec<u8> = row.try_get("data")?;
    let envelope: Envelope = serde_json::from_slice(&data)?;
    Ok(Paste {
        id: id.to_owned(),
        adata: envelope.adata,
        ct: envelope.ct,
        created: row.try_get("created_at")?,
        expire_at: row.try_get("expire_at")?,
    })
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn create_paste(&self, paste: &Paste) -> StorageResult<()> {
        let data = serde_json::to_vec(&EnvelopeRef::new(&paste.adata, &paste.ct))?;
        sqlx::query(
            "INSERT INTO pastes (id, data, expire_at, burn, discussion, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&paste.id)
        .bind(data)
        .bind(paste.expire_at)
        .bind(paste.burn_after_reading())
        .bind(paste.discussion())
        .bind(paste.created)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_paste(&self, id: &str) -> StorageResult<Option<Paste>> {
        sqlx::query("SELECT data, expire_at, created_at FROM pastes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| paste_from_row(id, &row))
            .transpose()
    }

    async fn read_paste(&self, id: &str) -> StorageResult<Option<(Paste, Vec<Comment>)>> {
        let Some(row) = sqlx::query("SELECT burn FROM pastes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };

        if row.try_get::<bool, _>("burn")? {
            // A single DELETE ... RETURNING statement is atomic: only one
            // concurrent reader gets the row back.
            return sqlx::query(
                "DELETE FROM pastes WHERE id = ? RETURNING data, expire_at, created_at",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| Ok((paste_from_row(id, &row)?, Vec::new())))
            .transpose();
        }

        let Some(paste) = self.get_paste(id).await? else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT id, parent_id, data, created_at FROM comments \
             WHERE paste_id = ? ORDER BY created_at, rowid",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let comments = rows
            .iter()
            .map(|row| {
                let data: Vec<u8> = row.try_get("data")?;
                let envelope: Envelope = serde_json::from_slice(&data)?;
                Ok(Comment {
                    id: row.try_get("id")?,
                    paste_id: id.to_owned(),
                    parent_id: row.try_get("parent_id")?,
                    adata: envelope.adata,
                    ct: envelope.ct,
                    created: row.try_get("created_at")?,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(Some((paste, comments)))
    }

    async fn delete_paste(&self, id: &str) -> StorageResult<bool> {
        let result = sqlx::query("DELETE FROM pastes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn create_comment(&self, comment: &Comment) -> StorageResult<()> {
        let data = serde_json::to_vec(&EnvelopeRef::new(&comment.adata, &comment.ct))?;
        sqlx::query(
            "INSERT INTO comments (id, paste_id, parent_id, data, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&comment.id)
        .bind(&comment.paste_id)
        .bind(&comment.parent_id)
        .bind(data)
        .bind(comment.created)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn count_comments(&self, paste_id: &str) -> StorageResult<u64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM comments WHERE paste_id = ?")
            .bind(paste_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    async fn comment_exists(&self, paste_id: &str, comment_id: &str) -> StorageResult<bool> {
        let row = sqlx::query("SELECT 1 FROM comments WHERE id = ? AND paste_id = ?")
            .bind(comment_id)
            .bind(paste_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    async fn purge_expired(&self, now: i64) -> StorageResult<u64> {
        let result =
            sqlx::query("DELETE FROM pastes WHERE expire_at IS NOT NULL AND expire_at <= ?")
                .bind(now)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }
}
