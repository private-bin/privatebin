use std::net::IpAddr;

use serde_json::{Map, Value, json};

use super::create::{ID_ATTEMPTS, check_rate, limits};
use crate::error::ApiError;
use crate::model::{Comment, generate_id, validate_comment};
use crate::state::AppState;
use crate::storage::StorageError;

pub async fn create(
    state: &AppState,
    ip: IpAddr,
    body: Map<String, Value>,
) -> Result<Value, ApiError> {
    if !state.config.main.discussion {
        return Err(ApiError::DiscussionDisabled);
    }
    check_rate(state, ip)?;
    let mut limits = limits(state);
    limits.size_limit = limits.size_limit.min(state.config.main.comment_size_limit);
    let new = validate_comment(body, limits).map_err(|e| match e {
        ApiError::TooLarge(limit) => ApiError::CommentTooLarge(limit),
        other => other,
    })?;
    let now = state.now();

    let storage_error = |e: StorageError| {
        tracing::error!(error = %e, "failed to access storage for comment");
        ApiError::SaveComment
    };
    // As in PrivateBin, every reason a paste cannot take this comment is
    // reported as invalid data.
    let paste = state
        .storage
        .get_paste(&new.paste_id)
        .await
        .map_err(storage_error)?
        .filter(|p| !p.is_expired(now))
        .ok_or(ApiError::InvalidData)?;
    if !paste.discussion() || paste.burn_after_reading() {
        return Err(ApiError::InvalidData);
    }
    let comments = state
        .storage
        .count_comments(&new.paste_id)
        .await
        .map_err(storage_error)?;
    if comments >= state.config.main.max_comments {
        return Err(ApiError::TooManyComments);
    }
    if new.parent_id != new.paste_id
        && !state
            .storage
            .comment_exists(&new.paste_id, &new.parent_id)
            .await
            .map_err(storage_error)?
    {
        return Err(ApiError::InvalidData);
    }

    let mut comment = Comment {
        id: String::new(),
        paste_id: new.paste_id,
        parent_id: new.parent_id,
        adata: new.adata,
        ct: new.ct,
        created: now,
    };
    for _ in 0..ID_ATTEMPTS {
        comment.id = generate_id();
        match state.storage.create_comment(&comment).await {
            Ok(()) => {
                tracing::debug!("comment created");
                return Ok(json!({
                    "status": 0,
                    "id": comment.id,
                    "url": format!("/?{}#{}", comment.paste_id, comment.id),
                }));
            }
            Err(StorageError::AlreadyExists) => continue,
            Err(e) => return Err(storage_error(e)),
        }
    }
    Err(ApiError::SaveComment)
}
