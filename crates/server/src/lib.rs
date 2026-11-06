//! PrivateBin server: a zero-knowledge pastebin speaking the PrivateBin JSON
//! API v2. The server only ever sees ciphertext.

pub mod config;
pub mod error;
pub mod healthcheck;
pub mod limiter;
pub mod model;
pub mod purge;
pub mod routes;
pub mod security;
pub mod state;
pub mod storage;

pub use config::Config;
pub use routes::router;
pub use state::AppState;

/// Opens storage and resolves the server salt.
pub async fn build_state(config: Config) -> Result<AppState, Box<dyn std::error::Error>> {
    let salt = state::resolve_salt(&config)
        .map_err(|e| format!("cannot load or create the server salt: {e}"))?;
    let storage = storage::open(&config.storage).await?;
    Ok(AppState::new(config, storage, salt))
}
