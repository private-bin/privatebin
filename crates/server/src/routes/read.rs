use serde_json::{Map, Value, json};

use super::response::paste_url;
use crate::error::ApiError;
use crate::model::{Comment, is_valid_id};
use crate::state::AppState;

pub async fn read(state: &AppState, id: &str) -> Result<Value, ApiError> {
    if !is_valid_id(id) {
        return Err(ApiError::InvalidPasteId);
    }
    let found = state.storage.read_paste(id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to read paste");
        ApiError::PasteNotFound
    })?;
    let Some((paste, comments)) = found else {
        return Err(ApiError::PasteNotFound);
    };
    let now = state.now();
    if paste.is_expired(now) {
        // Lazy expiry: the background purge may not have run yet.
        if let Err(e) = state.storage.delete_paste(id).await {
            tracing::error!(error = %e, "failed to delete expired paste");
        }
        return Err(ApiError::PasteNotFound);
    }

    let mut meta = Map::new();
    meta.insert("created".into(), paste.created.into());
    if let Some(expire_at) = paste.expire_at {
        meta.insert("time_to_live".into(), (expire_at - now).into());
    }
    let comment_count = comments.len();
    let comments: Vec<Value> = comments.into_iter().map(comment_json).collect();

    Ok(json!({
        "status": 0,
        "id": paste.id,
        "url": paste_url(&paste.id),
        "v": 2,
        "adata": paste.adata,
        "ct": paste.ct,
        "meta": meta,
        "comments": comments,
        "comment_count": comment_count,
        "comment_offset": 0,
        "@context": "?jsonld=paste",
    }))
}

fn comment_json(c: Comment) -> Value {
    json!({
        "v": 2,
        "adata": c.adata,
        "ct": c.ct,
        "id": c.id,
        "pasteid": c.paste_id,
        "parentid": c.parent_id,
        "meta": { "created": c.created },
    })
}
