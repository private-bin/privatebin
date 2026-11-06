//! Filesystem backend, laid out like the original PrivateBin:
//!
//! ```text
//! <root>/<ab>/<cd>/<id>.json                       paste
//! <root>/<ab>/<cd>/<id>.discussion/<cid>.<pid>.json comment <cid>, reply to <pid>
//! ```
//!
//! Records are written to a temporary file and then hard-linked into place,
//! which is atomic and fails if the target already exists.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use super::{EnvelopeRef, Storage, StorageError, StorageResult};
use crate::model::{Comment, Paste, generate_id, is_valid_id};

const PASTE_EXT: &str = ".json";
const DISCUSSION_EXT: &str = ".discussion";
const TMP_PREFIX: &str = ".tmp-";
/// Leftover temporary files older than this are removed by the purge.
const STALE_TMP_AGE: Duration = Duration::from_secs(3600);

pub struct FilesystemStorage {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct PasteMeta {
    created: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expire_date: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct CommentMeta {
    created: i64,
}

#[derive(Serialize)]
struct RecordRef<'a, M> {
    #[serde(flatten)]
    envelope: EnvelopeRef<'a>,
    meta: M,
}

#[derive(Deserialize)]
struct Record<M> {
    adata: Value,
    ct: String,
    meta: M,
}

/// Only the metadata, so the purge does not allocate the ciphertext.
#[derive(Deserialize)]
struct MetaOnly {
    meta: PasteMeta,
}

impl FilesystemStorage {
    pub async fn open(root: &Path) -> StorageResult<Self> {
        tokio::fs::create_dir_all(root).await?;
        Ok(Self {
            root: root.to_owned(),
        })
    }

    /// `<root>/<ab>/<cd>`. Callers must pass validated ids.
    fn shard_dir(&self, id: &str) -> PathBuf {
        debug_assert!(is_valid_id(id));
        self.root.join(&id[0..2]).join(&id[2..4])
    }

    fn paste_path(&self, id: &str) -> PathBuf {
        self.shard_dir(id).join(format!("{id}{PASTE_EXT}"))
    }

    fn discussion_dir(&self, id: &str) -> PathBuf {
        self.shard_dir(id).join(format!("{id}{DISCUSSION_EXT}"))
    }

    async fn read_paste_file(path: &Path, id: &str) -> StorageResult<Option<Paste>> {
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let record: Record<PasteMeta> = serde_json::from_slice(&bytes)?;
        Ok(Some(Paste {
            id: id.to_owned(),
            adata: record.adata,
            ct: record.ct,
            created: record.meta.created,
            expire_at: record.meta.expire_date,
        }))
    }

    async fn read_comments(&self, paste_id: &str) -> StorageResult<Vec<Comment>> {
        let mut entries = match tokio::fs::read_dir(self.discussion_dir(paste_id)).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut comments = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some((id, parent_id)) = name.to_str().and_then(parse_comment_name) else {
                continue;
            };
            let bytes = match tokio::fs::read(entry.path()).await {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let record: Record<CommentMeta> = serde_json::from_slice(&bytes)?;
            comments.push(Comment {
                id: id.to_owned(),
                paste_id: paste_id.to_owned(),
                parent_id: parent_id.to_owned(),
                adata: record.adata,
                ct: record.ct,
                created: record.meta.created,
            });
        }
        comments.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
        Ok(comments)
    }
}

/// `<cid>.<pid>.json` -> `(cid, pid)`
fn parse_comment_name(name: &str) -> Option<(&str, &str)> {
    let (id, parent) = name.strip_suffix(PASTE_EXT)?.split_once('.')?;
    (is_valid_id(id) && is_valid_id(parent)).then_some((id, parent))
}

/// Writes `bytes` to `target` atomically; fails with `AlreadyExists` if
/// `target` exists.
async fn write_new(target: &Path, bytes: &[u8]) -> StorageResult<()> {
    let dir = target.parent().expect("record paths have a parent");
    tokio::fs::create_dir_all(dir).await?;
    let tmp = dir.join(format!("{TMP_PREFIX}{}", generate_id()));
    let result = async {
        let mut file = tokio::fs::File::create_new(&tmp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::hard_link(&tmp, target).await
    }
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    match result {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(StorageError::AlreadyExists),
        Err(e) => Err(e.into()),
    }
}

async fn remove_dir_if_exists(path: &Path) -> StorageResult<()> {
    match tokio::fs::remove_dir_all(path).await {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[async_trait]
impl Storage for FilesystemStorage {
    async fn create_paste(&self, paste: &Paste) -> StorageResult<()> {
        let record = RecordRef {
            envelope: EnvelopeRef::new(&paste.adata, &paste.ct),
            meta: PasteMeta {
                created: paste.created,
                expire_date: paste.expire_at,
            },
        };
        write_new(&self.paste_path(&paste.id), &serde_json::to_vec(&record)?).await
    }

    async fn get_paste(&self, id: &str) -> StorageResult<Option<Paste>> {
        Self::read_paste_file(&self.paste_path(id), id).await
    }

    async fn read_paste(&self, id: &str) -> StorageResult<Option<(Paste, Vec<Comment>)>> {
        let path = self.paste_path(id);
        let Some(paste) = Self::read_paste_file(&path, id).await? else {
            return Ok(None);
        };
        if !paste.burn_after_reading() {
            let comments = self.read_comments(id).await?;
            return Ok(Some((paste, comments)));
        }

        // Claim the paste by renaming it: rename is atomic, so exactly one
        // concurrent reader succeeds; the others see NotFound.
        let claimed = self
            .shard_dir(id)
            .join(format!("{TMP_PREFIX}burn-{}", generate_id()));
        match tokio::fs::rename(&path, &claimed).await {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let paste = Self::read_paste_file(&claimed, id).await;
        let _ = tokio::fs::remove_file(&claimed).await;
        remove_dir_if_exists(&self.discussion_dir(id)).await?;
        Ok(paste?.map(|p| (p, Vec::new())))
    }

    async fn delete_paste(&self, id: &str) -> StorageResult<bool> {
        let existed = match tokio::fs::remove_file(self.paste_path(id)).await {
            Ok(()) => true,
            Err(e) if e.kind() == ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        remove_dir_if_exists(&self.discussion_dir(id)).await?;
        Ok(existed)
    }

    async fn create_comment(&self, comment: &Comment) -> StorageResult<()> {
        if self.comment_exists(&comment.paste_id, &comment.id).await? {
            return Err(StorageError::AlreadyExists);
        }
        let record = RecordRef {
            envelope: EnvelopeRef::new(&comment.adata, &comment.ct),
            meta: CommentMeta {
                created: comment.created,
            },
        };
        let path = self
            .discussion_dir(&comment.paste_id)
            .join(format!("{}.{}{PASTE_EXT}", comment.id, comment.parent_id));
        write_new(&path, &serde_json::to_vec(&record)?).await
    }

    async fn count_comments(&self, paste_id: &str) -> StorageResult<u64> {
        let mut entries = match tokio::fs::read_dir(self.discussion_dir(paste_id)).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut count = 0;
        while let Some(entry) = entries.next_entry().await? {
            if entry
                .file_name()
                .to_str()
                .and_then(parse_comment_name)
                .is_some()
            {
                count += 1;
            }
        }
        Ok(count)
    }

    async fn comment_exists(&self, paste_id: &str, comment_id: &str) -> StorageResult<bool> {
        let mut entries = match tokio::fs::read_dir(self.discussion_dir(paste_id)).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            if name
                .to_str()
                .and_then(parse_comment_name)
                .is_some_and(|(id, _)| id == comment_id)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn purge_expired(&self, now: i64) -> StorageResult<u64> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || purge_blocking(&root, now))
            .await
            .map_err(|e| StorageError::Backend(format!("purge task failed: {e}")))?
    }
}

/// Removes expired pastes. A file that cannot be read or removed is logged and
/// skipped, so one bad entry never stops expiry of the others.
fn purge_blocking(root: &Path, now: i64) -> StorageResult<u64> {
    let mut purged = 0;
    for shard in subdirs(root)?
        .iter()
        .flat_map(|d| subdirs(d).unwrap_or_default())
    {
        let entries = match std::fs::read_dir(&shard) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(error = %e, "skipping unreadable shard during purge");
                continue;
            }
        };
        let mut discussions = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with(TMP_PREFIX) {
                let stale = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age > STALE_TMP_AGE);
                if stale {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            if let Some(id) = name
                .strip_suffix(DISCUSSION_EXT)
                .filter(|id| is_valid_id(id))
            {
                discussions.push(id.to_owned());
                continue;
            }
            let Some(id) = name.strip_suffix(PASTE_EXT).filter(|id| is_valid_id(id)) else {
                continue;
            };
            match purge_paste_file(&shard, &path, id, now) {
                Ok(true) => purged += 1,
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "cannot purge paste file"),
            }
        }
        // A comment written while its paste was being deleted leaves a
        // discussion without a paste behind; nothing else would remove it.
        for id in discussions {
            if !shard.join(format!("{id}{PASTE_EXT}")).exists()
                && let Err(e) = std::fs::remove_dir_all(shard.join(format!("{id}{DISCUSSION_EXT}")))
                && e.kind() != ErrorKind::NotFound
            {
                tracing::warn!(error = %e, "cannot remove orphaned discussion");
            }
        }
    }
    Ok(purged)
}

/// Deletes one paste file (and its discussion) if it has expired.
fn purge_paste_file(shard: &Path, path: &Path, id: &str, now: i64) -> std::io::Result<bool> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let expired = match serde_json::from_reader::<_, MetaOnly>(std::io::BufReader::new(file)) {
        Ok(record) => record.meta.expire_date.is_some_and(|at| at <= now),
        Err(e) => {
            tracing::warn!(error = %e, "skipping unreadable paste file during purge");
            false
        }
    };
    if !expired {
        return Ok(false);
    }
    let removed = match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    match std::fs::remove_dir_all(shard.join(format!("{id}{DISCUSSION_EXT}"))) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
        _ => Ok(removed),
    }
}

/// Two-character shard directories directly below `dir`.
fn subdirs(dir: &Path) -> StorageResult<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let is_shard = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.len() == 2 && n.bytes().all(|b| b.is_ascii_hexdigit()));
        if is_shard && entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comment_file_names() {
        assert_eq!(
            parse_comment_name("0123456789abcdef.fedcba9876543210.json"),
            Some(("0123456789abcdef", "fedcba9876543210"))
        );
        assert_eq!(parse_comment_name(".tmp-0123456789abcdef"), None);
        assert_eq!(parse_comment_name("0123456789abcdef.json"), None);
    }
}
