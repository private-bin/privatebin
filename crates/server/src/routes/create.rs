use std::net::IpAddr;

use serde_json::{Map, Value, json};

use super::response::paste_url;
use crate::error::ApiError;
use crate::model::{Paste, ValidationLimits, delete_token, generate_id, validate_paste};
use crate::state::AppState;
use crate::storage::StorageError;

/// Attempts at finding an unused random identifier.
pub(super) const ID_ATTEMPTS: usize = 8;

pub(super) fn limits(state: &AppState) -> ValidationLimits {
    ValidationLimits {
        size_limit: state.config.main.size_limit,
        discussion: state.config.main.discussion,
    }
}

pub(super) fn check_rate(state: &AppState, ip: IpAddr) -> Result<(), ApiError> {
    if state.limiter.check(ip) {
        Ok(())
    } else {
        Err(ApiError::RateLimited(state.config.traffic.limit))
    }
}

pub async fn create(
    state: &AppState,
    ip: IpAddr,
    body: Map<String, Value>,
) -> Result<Value, ApiError> {
    check_rate(state, ip)?;
    let new = validate_paste(body, limits(state))?;
    let now = state.now();
    let lifetime = state.config.expire.lifetime(new.expire.as_deref());
    let mut paste = Paste {
        id: String::new(),
        adata: new.adata,
        ct: new.ct,
        created: now,
        expire_at: lifetime.map(|secs| now.saturating_add_unsigned(secs)),
    };

    for _ in 0..ID_ATTEMPTS {
        paste.id = generate_id();
        match state.storage.create_paste(&paste).await {
            Ok(()) => {
                tracing::debug!("paste created");
                return Ok(json!({
                    "status": 0,
                    "id": paste.id,
                    "url": paste_url(&paste.id),
                    "deletetoken": delete_token(&state.salt, &paste.id),
                }));
            }
            Err(StorageError::AlreadyExists) => continue,
            Err(e) => {
                tracing::error!(error = %e, "failed to store paste");
                return Err(ApiError::SavePaste);
            }
        }
    }
    tracing::error!("could not find an unused paste id");
    Err(ApiError::SavePaste)
}
