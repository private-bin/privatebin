//! Configuration: defaults, overridden by an optional TOML file, overridden by
//! `PRIVATEBIN__SECTION__KEY` environment variables.

use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use ipnet::IpNet;
use serde::{Deserialize, Deserializer};

/// Location of the configuration file when `--config` is not given.
pub const DEFAULT_CONFIG_PATH: &str = "/etc/privatebin/config.toml";

/// Prefix of environment variables overriding the configuration.
pub const ENV_PREFIX: &str = "PRIVATEBIN__";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub main: MainConfig,
    pub expire: ExpireConfig,
    pub traffic: TrafficConfig,
    pub purge: PurgeConfig,
    pub storage: StorageConfig,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address and port to listen on.
    pub bind: SocketAddr,
    /// HMAC key for delete tokens. When unset, a random key is generated once
    /// and persisted in `salt_file`.
    #[serde(deserialize_with = "deserialize_opt_string")]
    pub salt: Option<String>,
    /// Where the generated salt is persisted.
    pub salt_file: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 8080)),
            salt: None,
            salt_file: PathBuf::from("/data/salt"),
        }
    }
}

// Hand-written so the salt never ends up in logs.
impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind", &self.bind)
            .field("salt", &self.salt.as_ref().map(|_| "<redacted>"))
            .field("salt_file", &self.salt_file)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MainConfig {
    /// Name shown in the UI.
    #[serde(deserialize_with = "deserialize_string")]
    pub name: String,
    /// Maximum length of the encrypted payload (`ct`), in bytes.
    pub size_limit: u64,
    pub discussion: bool,
    /// Maximum length of an encrypted comment (`ct`), in bytes. Comments are
    /// text only; together with `max_comments` this bounds the size of a paste.
    pub comment_size_limit: u64,
    /// Maximum number of comments per paste.
    pub max_comments: u64,
    pub password: bool,
    pub file_upload: bool,
    pub burn_after_reading_selected: bool,
    pub default_formatter: Formatter,
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            name: "PrivateBin".to_owned(),
            size_limit: 10 * 1024 * 1024,
            discussion: true,
            comment_size_limit: 64 * 1024,
            max_comments: 500,
            password: true,
            file_upload: true,
            burn_after_reading_selected: false,
            default_formatter: Formatter::Plaintext,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Formatter {
    Plaintext,
    Syntaxhighlighting,
    Markdown,
}

impl Formatter {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "plaintext" => Some(Self::Plaintext),
            "syntaxhighlighting" => Some(Self::Syntaxhighlighting),
            "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExpireConfig {
    /// Key of the option selected by default.
    pub default: String,
    /// Expiration options: key -> lifetime in seconds (0 = never). They are
    /// presented ordered by lifetime, with "never" last.
    pub options: BTreeMap<String, u64>,
}

impl Default for ExpireConfig {
    fn default() -> Self {
        let options = [
            ("5min", 300),
            ("10min", 600),
            ("1hour", 3600),
            ("1day", 86_400),
            ("1week", 604_800),
            ("1month", 2_592_000),
            ("1year", 31_536_000),
            ("never", 0),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        Self {
            default: "1week".to_owned(),
            options,
        }
    }
}

impl ExpireConfig {
    /// Options ordered by lifetime, "never" (0) last.
    pub fn ordered(&self) -> Vec<(&str, u64)> {
        let mut options: Vec<(&str, u64)> =
            self.options.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        options.sort_by_key(|&(_, secs)| if secs == 0 { u64::MAX } else { secs });
        options
    }

    /// Lifetime in seconds for an expire key, falling back to the default
    /// option for unknown keys. `None` means the paste never expires.
    pub fn lifetime(&self, key: Option<&str>) -> Option<u64> {
        let secs = key
            .and_then(|k| self.options.get(k))
            .or_else(|| self.options.get(&self.default))
            .copied()
            .unwrap_or(0);
        (secs > 0).then_some(secs)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrafficConfig {
    /// Minimum number of seconds between two posts from the same IP; 0 disables.
    pub limit: u64,
    /// Proxies whose `X-Forwarded-For` header is trusted. Accepts a TOML array
    /// or a comma-separated string (handy for environment variables).
    #[serde(deserialize_with = "deserialize_nets")]
    pub trusted_proxies: Vec<IpNet>,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            limit: 10,
            trusted_proxies: vec![
                "127.0.0.1/32".parse().expect("valid CIDR"),
                "172.16.0.0/12".parse().expect("valid CIDR"),
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PurgeConfig {
    /// Seconds between two purges of expired pastes; 0 disables the task.
    pub interval_secs: u64,
}

impl Default for PurgeConfig {
    fn default() -> Self {
        Self { interval_secs: 300 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    Sqlite,
    Filesystem,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    pub backend: StorageBackend,
    pub sqlite_path: PathBuf,
    pub filesystem_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            sqlite_path: PathBuf::from("/data/privatebin.db"),
            filesystem_dir: PathBuf::from("/data/pastes"),
        }
    }
}

/// Accepts numbers too: env values such as `PRIVATEBIN__SERVER__SALT=1234`
/// are parsed as integers by figment.
fn deserialize_opt_string<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Str(String),
        Int(i128),
        Float(f64),
        Bool(bool),
    }
    Ok(Option::<Raw>::deserialize(de)?.map(|raw| match raw {
        Raw::Str(s) => s,
        Raw::Int(i) => i.to_string(),
        Raw::Float(f) => f.to_string(),
        Raw::Bool(b) => b.to_string(),
    }))
}

/// Environment variables are typed by their content (`NAME=2024` is a number),
/// so string settings accept scalars too.
fn deserialize_string<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    deserialize_opt_string(de)?.ok_or_else(|| serde::de::Error::custom("expected a string"))
}

fn deserialize_nets<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<IpNet>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Nets {
        List(Vec<String>),
        Csv(String),
    }
    let items = match Nets::deserialize(de)? {
        Nets::List(list) => list,
        Nets::Csv(csv) => csv.split(',').map(str::to_owned).collect(),
    };
    items
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            // Accept bare addresses as single-host networks.
            s.parse::<IpNet>()
                .or_else(|_| s.parse::<std::net::IpAddr>().map(IpNet::from))
                .map_err(|_| serde::de::Error::custom(format!("invalid CIDR: {s}")))
        })
        .collect()
}

#[derive(Debug)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Loads the configuration. An explicitly given file must exist; the
    /// default file is optional so the container can run on env vars alone.
    pub fn load(explicit: Option<&Path>) -> Result<Self, ConfigError> {
        let mut figment = Figment::new();
        match explicit {
            Some(path) => {
                if !path.is_file() {
                    return Err(ConfigError(format!(
                        "config file not found: {}",
                        path.display()
                    )));
                }
                figment = figment.merge(Toml::file_exact(path));
            }
            None => {
                let path = Path::new(DEFAULT_CONFIG_PATH);
                if path.is_file() {
                    figment = figment.merge(Toml::file_exact(path));
                }
            }
        }
        Self::from_figment(figment.merge(Env::prefixed(ENV_PREFIX).split("__")))
    }

    pub fn from_figment(figment: Figment) -> Result<Self, ConfigError> {
        let config: Config = figment
            .extract()
            .map_err(|e| ConfigError(format!("invalid configuration: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.expire.options.is_empty() {
            return Err(ConfigError("expire.options must not be empty".into()));
        }
        if !self.expire.options.contains_key(&self.expire.default) {
            return Err(ConfigError(format!(
                "expire.default {:?} is not one of expire.options",
                self.expire.default
            )));
        }
        if self.main.size_limit == 0 {
            return Err(ConfigError("main.size_limit must be positive".into()));
        }
        if self.main.comment_size_limit == 0 {
            return Err(ConfigError(
                "main.comment_size_limit must be positive".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::Jail;

    #[test]
    fn defaults() {
        let c = Config::from_figment(Figment::new()).unwrap();
        assert_eq!(c.server.bind.port(), 8080);
        assert_eq!(c.main.size_limit, 10_485_760);
        assert_eq!(c.expire.default, "1week");
        assert_eq!(c.traffic.trusted_proxies.len(), 2);
        let keys: Vec<_> = c.expire.ordered().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            [
                "5min", "10min", "1hour", "1day", "1week", "1month", "1year", "never"
            ]
        );
    }

    #[test]
    fn lifetime_falls_back_to_default() {
        let e = ExpireConfig::default();
        assert_eq!(e.lifetime(Some("5min")), Some(300));
        assert_eq!(e.lifetime(Some("never")), None);
        assert_eq!(e.lifetime(Some("bogus")), Some(604_800));
        assert_eq!(e.lifetime(None), Some(604_800));
    }

    #[test]
    #[allow(clippy::result_large_err)] // figment's Jail API
    fn env_overrides_toml() {
        Jail::expect_with(|jail| {
            jail.create_file(
                "c.toml",
                r#"
                [main]
                size_limit = 2048
                [traffic]
                trusted_proxies = ["10.0.0.0/8"]
                [expire]
                default = "1day"
                options = { "1day" = 86400, "never" = 0 }
                "#,
            )?;
            jail.set_env("PRIVATEBIN__MAIN__DISCUSSION", "false");
            jail.set_env("PRIVATEBIN__STORAGE__BACKEND", "filesystem");
            jail.set_env(
                "PRIVATEBIN__TRAFFIC__TRUSTED_PROXIES",
                "172.16.0.0/12, 127.0.0.1",
            );
            let c = Config::load(Some(Path::new("c.toml"))).unwrap();
            assert_eq!(c.main.size_limit, 2048);
            assert!(!c.main.discussion);
            assert_eq!(c.storage.backend, StorageBackend::Filesystem);
            assert_eq!(c.expire.options.len(), 2);
            assert_eq!(
                c.traffic.trusted_proxies,
                vec![
                    "172.16.0.0/12".parse::<IpNet>().unwrap(),
                    "127.0.0.1/32".parse().unwrap()
                ]
            );
            Ok(())
        });
    }

    #[test]
    fn rejects_unknown_default_expire() {
        let figment = Figment::new().merge(figment::providers::Serialized::default(
            "expire.default",
            "2weeks",
        ));
        assert!(Config::from_figment(figment).is_err());
    }
}
