//! Storage backends. Pastes and comments are opaque ciphertext to the server;
//! backends only persist them and enforce atomicity guarantees.

mod filesystem;
mod sqlite;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

pub use filesystem::FilesystemStorage;
pub use sqlite::SqliteStorage;

use crate::config::{StorageBackend, StorageConfig};
use crate::model::{Comment, Paste};

#[derive(Debug)]
pub enum StorageError {
    /// A paste or comment with the same identifier already exists.
    AlreadyExists,
    /// Any backend failure (I/O, database, corrupt record).
    Backend(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists => f.write_str("record already exists"),
            Self::Backend(msg) => write!(f, "storage backend error: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Backend(e.to_string())
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Backend(format!("corrupt record: {e}"))
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Stores a new paste; fails with [`StorageError::AlreadyExists`] if the id is taken.
    async fn create_paste(&self, paste: &Paste) -> StorageResult<()>;

    /// Returns a paste without side effects (no burn), without comments.
    async fn get_paste(&self, id: &str) -> StorageResult<Option<Paste>>;

    /// Reads a paste and its comments (sorted by creation time). A
    /// burn-after-reading paste is deleted atomically: concurrent readers can
    /// never both obtain it.
    async fn read_paste(&self, id: &str) -> StorageResult<Option<(Paste, Vec<Comment>)>>;

    /// Deletes a paste and its comments. Returns whether it existed.
    async fn delete_paste(&self, id: &str) -> StorageResult<bool>;

    /// Stores a new comment; fails with [`StorageError::AlreadyExists`] if the id is taken.
    async fn create_comment(&self, comment: &Comment) -> StorageResult<()>;

    /// Number of comments stored for paste `paste_id`.
    async fn count_comments(&self, paste_id: &str) -> StorageResult<u64>;

    /// Whether `comment_id` is a comment of paste `paste_id`.
    async fn comment_exists(&self, paste_id: &str, comment_id: &str) -> StorageResult<bool>;

    /// Deletes every paste whose expiry is at or before `now`. Returns the count.
    async fn purge_expired(&self, now: i64) -> StorageResult<u64>;
}

/// Opens the backend selected in the configuration.
pub async fn open(config: &StorageConfig) -> StorageResult<Arc<dyn Storage>> {
    Ok(match config.backend {
        StorageBackend::Sqlite => Arc::new(SqliteStorage::open(&config.sqlite_path).await?),
        StorageBackend::Filesystem => {
            Arc::new(FilesystemStorage::open(&config.filesystem_dir).await?)
        }
    })
}

/// JSON shape persisted for paste and comment payloads:
/// `{"v":2,"adata":...,"ct":...}`.
#[derive(serde::Serialize)]
struct EnvelopeRef<'a> {
    v: u8,
    adata: &'a serde_json::Value,
    ct: &'a str,
}

impl<'a> EnvelopeRef<'a> {
    fn new(adata: &'a serde_json::Value, ct: &'a str) -> Self {
        Self { v: 2, adata, ct }
    }
}

#[derive(serde::Deserialize)]
struct Envelope {
    adata: serde_json::Value,
    ct: String,
}
