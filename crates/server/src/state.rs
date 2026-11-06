//! Shared application state.

use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::limiter::RateLimiter;
use crate::routes::static_files;
use crate::storage::Storage;

/// Returns the current unix time in seconds. Injectable for tests.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    })
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
    pub limiter: Arc<RateLimiter>,
    /// HMAC key for delete tokens.
    pub salt: Arc<[u8]>,
    /// `index.html` with the public configuration injected.
    pub index_html: Arc<str>,
    pub clock: Clock,
}

impl AppState {
    pub fn new(config: Config, storage: Arc<dyn Storage>, salt: Vec<u8>) -> Self {
        let index_html = static_files::render_index(&config).into();
        let limiter = RateLimiter::new(Duration::from_secs(config.traffic.limit));
        Self {
            config: Arc::new(config),
            storage,
            limiter: Arc::new(limiter),
            salt: salt.into(),
            index_html,
            clock: system_clock(),
        }
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn now(&self) -> i64 {
        (self.clock)()
    }
}

/// Returns the configured salt, or the one persisted in `salt_file`,
/// generating and persisting 32 random bytes (hex) on first start.
pub fn resolve_salt(config: &Config) -> std::io::Result<Vec<u8>> {
    if let Some(salt) = config.server.salt.as_deref().filter(|s| !s.is_empty()) {
        return Ok(salt.as_bytes().to_vec());
    }
    load_or_create_salt(&config.server.salt_file)
}

fn load_or_create_salt(path: &Path) -> std::io::Result<Vec<u8>> {
    match std::fs::read_to_string(path) {
        Ok(salt) if !salt.trim().is_empty() => return Ok(salt.trim().as_bytes().to_vec()),
        Ok(_) => {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("salt file {} is empty", path.display()),
            ));
        }
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let salt = hex::encode(bytes);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(salt.as_bytes())?;
            file.sync_all()?;
            tracing::info!(path = %path.display(), "generated a new server salt");
            Ok(salt.into_bytes())
        }
        // Another process won the race: use its salt.
        Err(e) if e.kind() == ErrorKind::AlreadyExists => load_or_create_salt(path),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salt_is_generated_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/salt");
        let first = load_or_create_salt(&path).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(load_or_create_salt(&path).unwrap(), first);
    }

    #[test]
    fn configured_salt_wins() {
        let mut config = Config::default();
        config.server.salt = Some("secret".into());
        assert_eq!(resolve_salt(&config).unwrap(), b"secret");
    }
}
