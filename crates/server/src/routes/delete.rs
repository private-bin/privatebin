use serde_json::{Value, json};

use super::response::paste_url;
use crate::error::ApiError;
use crate::model::{is_valid_id, verify_delete_token};
use crate::state::AppState;

pub async fn delete(state: &AppState, id: &str, token: &str) -> Result<Value, ApiError> {
    if !is_valid_id(id) {
        return Err(ApiError::InvalidPasteId);
    }
    let paste = state.storage.get_paste(id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to read paste");
        ApiError::PasteNotFound
    })?;
    let Some(paste) = paste else {
        return Err(ApiError::PasteNotFound);
    };
    if paste.is_expired(state.now()) {
        let _ = state.storage.delete_paste(id).await;
        return Err(ApiError::PasteNotFound);
    }
    if !verify_delete_token(&state.salt, id, token) {
        return Err(ApiError::WrongDeleteToken);
    }
    match state.storage.delete_paste(id).await {
        Ok(_) => {
            tracing::debug!("paste deleted");
            Ok(json!({ "status": 0, "id": id, "url": paste_url(id) }))
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to delete paste");
            Err(ApiError::PasteNotFound)
        }
    }
}
