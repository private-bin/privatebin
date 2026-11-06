//! The embedded web application (`web/dist`).

use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;
use serde::Serialize;

use crate::config::{Config, Formatter};
use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebDist;

/// Placeholder in `index.html` replaced by the public configuration.
const CONFIG_PLACEHOLDER: &str = "<!--PB_CONFIG-->";
/// Hashed assets never change under the same name.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Unhashed root files (favicon, robots.txt).
const SHORT_CACHE: &str = "public, max-age=3600";

/// Configuration exposed to the web application.
#[derive(Serialize)]
struct PublicConfig<'a> {
    name: &'a str,
    version: &'a str,
    size_limit: u64,
    discussion: bool,
    password: bool,
    file_upload: bool,
    burn_after_reading_selected: bool,
    default_formatter: Formatter,
    expire: PublicExpire<'a>,
}

#[derive(Serialize)]
struct PublicExpire<'a> {
    default: &'a str,
    options: Vec<PublicExpireOption<'a>>,
}

#[derive(Serialize)]
struct PublicExpireOption<'a> {
    key: &'a str,
    /// Lifetime in seconds, 0 = never.
    seconds: u64,
}

pub fn public_config_json(config: &Config) -> String {
    let public = PublicConfig {
        name: &config.main.name,
        version: env!("PRIVATEBIN_VERSION"),
        size_limit: config.main.size_limit,
        discussion: config.main.discussion,
        password: config.main.password,
        file_upload: config.main.file_upload,
        burn_after_reading_selected: config.main.burn_after_reading_selected,
        default_formatter: config.main.default_formatter,
        expire: PublicExpire {
            default: &config.expire.default,
            options: config
                .expire
                .ordered()
                .into_iter()
                .map(|(key, seconds)| PublicExpireOption { key, seconds })
                .collect(),
        },
    };
    let json = serde_json::to_string(&public).expect("config serializes");
    // Safe inside <script type="application/json">: these characters can
    // only occur inside JSON strings, where the escapes are equivalent.
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// `index.html` with the public configuration injected.
pub fn render_index(config: &Config) -> String {
    let template = WebDist::get("index.html")
        .map(|f| String::from_utf8_lossy(&f.data).into_owned())
        .unwrap_or_else(|| {
            tracing::warn!("web/dist/index.html is missing; serving a stub page");
            format!("<!doctype html><title>PrivateBin</title>{CONFIG_PLACEHOLDER}")
        });
    template.replace(CONFIG_PLACEHOLDER, &public_config_json(config))
}

pub fn index(state: &AppState) -> Response {
    Html(state.index_html.to_string()).into_response()
}

pub async fn asset(Path(path): Path<String>) -> Response {
    serve(&format!("assets/{path}"), IMMUTABLE)
}

/// Root-level files of `web/dist` (favicon etc.); everything else is a 404.
pub async fn fallback(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() || path.contains('/') || path == "index.html" {
        return not_found();
    }
    serve(path, SHORT_CACHE)
}

fn serve(path: &str, cache_control: &'static str) -> Response {
    let Some(file) = WebDist::get(path) else {
        return not_found();
    };
    let mime = file.metadata.mimetype();
    let mut response = file.data.into_owned().into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_config_is_escaped_and_ordered() {
        let mut config = Config::default();
        config.main.name = "</script><b>&".into();
        let json = public_config_json(&config);
        assert!(!json.contains('<') && !json.contains('>') && !json.contains('&'));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["name"], "</script><b>&");
        assert_eq!(value["expire"]["default"], "1week");
        assert_eq!(value["expire"]["options"][0]["key"], "5min");
        assert_eq!(value["expire"]["options"][7]["key"], "never");
        assert_eq!(value["size_limit"], 10_485_760);
        assert_eq!(value["default_formatter"], "plaintext");
    }
}
