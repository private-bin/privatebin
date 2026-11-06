//! HTTP routing. Like PrivateBin, everything API-related happens on `/`:
//! the operation is chosen from the query string (GET) or the JSON body (POST).

mod comment;
mod create;
mod delete;
mod jsonld;
mod read;
mod response;
pub mod static_files;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, RawQuery, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::value::RawValue;
use serde_json::{Map, Value};
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::timeout::TimeoutLayer;

use crate::error::ApiError;
use crate::limiter::client_ip;
use crate::security;
use crate::state::AppState;

/// Allowance on top of `size_limit` for the non-ciphertext parts of a body.
const BODY_OVERHEAD: usize = 64 * 1024;

/// API requests handled at once. Each one may buffer a body of up to
/// `size_limit` (or a read response with many comments); further requests
/// wait without their body being read.
const MAX_CONCURRENT_API_REQUESTS: usize = 16;

/// Upper bound for handling one request, body upload included. Slow clients
/// cannot hold a request (and its buffered body) open indefinitely.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Largest JSON value accepted for any POST field other than `ct`.
const MAX_FIELD_BYTES: usize = 4096;

pub fn router(state: AppState) -> Router {
    let body_limit = usize::try_from(state.config.main.size_limit)
        .unwrap_or(usize::MAX)
        .saturating_add(BODY_OVERHEAD);
    let router = Router::new()
        .route(
            "/",
            get(root_get).post(root_post).options(preflight).layer(
                GlobalConcurrencyLimitLayer::new(MAX_CONCURRENT_API_REQUESTS),
            ),
        )
        .route("/healthz", get(healthz))
        .route("/assets/{*path}", get(static_files::asset))
        .fallback(static_files::fallback)
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .with_state(state);
    security::apply(router)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn preflight() -> Response {
    response::with_cors(StatusCode::OK.into_response())
}

/// Request parameters understood on `GET /`.
#[derive(Debug, PartialEq, Eq)]
enum GetQuery<'a> {
    Index,
    Read(&'a str),
    Delete { id: &'a str, token: &'a str },
    JsonLd(&'a str),
}

/// Parses `?<id>`, `?pasteid=<id>` and `?pasteid=<id>&deletetoken=<token>`.
fn parse_query(raw: &str) -> GetQuery<'_> {
    let pairs: Vec<(&str, Option<&str>)> = raw
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (p, None),
        })
        .collect();
    let get = |key: &str| pairs.iter().find(|(k, _)| *k == key).and_then(|(_, v)| *v);
    if let Some(kind) = get("jsonld") {
        return GetQuery::JsonLd(kind);
    }
    match (get("pasteid"), get("deletetoken")) {
        (Some(id), Some(token)) => GetQuery::Delete { id, token },
        (Some(id), None) => GetQuery::Read(id),
        _ => match pairs.first() {
            Some((id, None | Some(""))) if !id.is_empty() => GetQuery::Read(id),
            _ => GetQuery::Index,
        },
    }
}

/// Whether the client wants the JSON API rather than the web application.
fn wants_json(headers: &HeaderMap) -> bool {
    if headers
        .get("x-requested-with")
        .is_some_and(|v| v.as_bytes() == b"JSONHttpRequest")
    {
        return true;
    }
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept.contains("application/json")
                && !accept.contains("text/html")
                && !accept.contains("application/xhtml+xml")
        })
}

async fn root_get(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    let query = query.as_deref().map_or(GetQuery::Index, parse_query);
    if let GetQuery::JsonLd(kind) = query {
        return jsonld::jsonld(&state, kind);
    }
    // Browsers get the web application, which repeats the request as JSON.
    // Deleting only on the JSON request keeps link previews from deleting.
    if !wants_json(&headers) {
        return static_files::index(&state);
    }
    let result = match query {
        GetQuery::Index | GetQuery::JsonLd(_) => return static_files::index(&state),
        GetQuery::Read(id) => read::read(&state, id).await,
        GetQuery::Delete { id, token } => delete::delete(&state, id, token).await,
    };
    response::into_response(result)
}

async fn root_post(
    State(state): State<AppState>,
    PeerIp(peer): PeerIp,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let result = async {
        let body = body.map_err(|rejection| {
            if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::TooLarge(state.config.main.size_limit)
            } else {
                ApiError::InvalidData
            }
        })?;
        let body = parse_body(&body)?;
        let ip = client_ip(peer, &headers, &state.config.traffic.trusted_proxies);
        dispatch_post(&state, ip, body).await
    }
    .await;
    response::into_response(result)
}

/// The POST fields understood by the API. Unknown keys are skipped without
/// being materialized.
#[derive(serde::Deserialize)]
struct RawBody<'a> {
    #[serde(borrow, default)]
    v: Option<&'a RawValue>,
    #[serde(borrow, default)]
    adata: Option<&'a RawValue>,
    #[serde(borrow, default)]
    ct: Option<&'a RawValue>,
    #[serde(borrow, default)]
    meta: Option<&'a RawValue>,
    #[serde(borrow, default)]
    pasteid: Option<&'a RawValue>,
    #[serde(borrow, default)]
    parentid: Option<&'a RawValue>,
    #[serde(borrow, default)]
    deletetoken: Option<&'a RawValue>,
}

/// Parses a POST body into its known fields. Parsing an arbitrary body into a
/// generic `Value` would let a dense 10 MB array (`[0,0,0,...]`) cost ~50x its
/// size in memory, so every field is size-checked before it is parsed:
/// `ct` must be a JSON string and the other fields are small.
fn parse_body(bytes: &[u8]) -> Result<Map<String, Value>, ApiError> {
    // A derived struct would also accept a top-level array.
    if bytes.trim_ascii_start().first() != Some(&b'{') {
        return Err(ApiError::InvalidData);
    }
    let raw: RawBody<'_> = serde_json::from_slice(bytes).map_err(|_| ApiError::InvalidData)?;
    let fields = [
        ("v", raw.v),
        ("adata", raw.adata),
        ("ct", raw.ct),
        ("meta", raw.meta),
        ("pasteid", raw.pasteid),
        ("parentid", raw.parentid),
        ("deletetoken", raw.deletetoken),
    ];
    let mut body = Map::new();
    for (key, value) in fields {
        let Some(value) = value else { continue };
        let text = value.get();
        let acceptable = if key == "ct" {
            text.starts_with('"')
        } else {
            text.len() <= MAX_FIELD_BYTES
        };
        if !acceptable {
            return Err(ApiError::InvalidData);
        }
        let value = serde_json::from_str(text).map_err(|_| ApiError::InvalidData)?;
        body.insert(key.to_owned(), value);
    }
    Ok(body)
}

async fn dispatch_post(
    state: &AppState,
    ip: IpAddr,
    body: Map<String, Value>,
) -> Result<Value, ApiError> {
    let has = |key: &str| body.get(key).is_some_and(|v| !v.is_null());
    if has("pasteid") && has("deletetoken") {
        let id = body["pasteid"].as_str().ok_or(ApiError::InvalidPasteId)?;
        let token = body["deletetoken"].as_str().unwrap_or_default();
        delete::delete(state, id, token).await
    } else if has("pasteid") && has("parentid") {
        comment::create(state, ip, body).await
    } else {
        create::create(state, ip, body).await
    }
}

/// TCP peer address; absent when the router is driven without a socket
/// (tests), in which case the unspecified address is used.
struct PeerIp(IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for PeerIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |ci| ci.0.ip()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_queries() {
        assert_eq!(parse_query(""), GetQuery::Index);
        assert_eq!(
            parse_query("0123456789abcdef"),
            GetQuery::Read("0123456789abcdef")
        );
        assert_eq!(
            parse_query("0123456789abcdef="),
            GetQuery::Read("0123456789abcdef")
        );
        assert_eq!(parse_query("pasteid=abc"), GetQuery::Read("abc"));
        assert_eq!(
            parse_query("pasteid=abc&deletetoken=def"),
            GetQuery::Delete {
                id: "abc",
                token: "def"
            }
        );
        assert_eq!(
            parse_query("deletetoken=def&pasteid=abc"),
            GetQuery::Delete {
                id: "abc",
                token: "def"
            }
        );
        assert_eq!(parse_query("jsonld=paste"), GetQuery::JsonLd("paste"));
    }

    #[test]
    fn parses_known_body_fields_only() {
        let body = parse_body(br#"{"v":2,"ct":"YWJj","adata":[1],"junk":[0,0,0],"pasteid":null}"#)
            .unwrap();
        assert_eq!(body.keys().collect::<Vec<_>>(), ["v", "adata", "ct"]);
        assert!(parse_body(br#"[2,"YWJj"]"#).is_err());
        assert!(parse_body(br#"{"ct":[0,0,0]}"#).is_err());
        let big = format!(r#"{{"adata":[{}0]}}"#, "0,".repeat(MAX_FIELD_BYTES));
        assert!(parse_body(big.as_bytes()).is_err());
        assert!(parse_body(br#"{"v":2,"v":2}"#).is_err());
        assert!(parse_body(b"not json").is_err());
    }

    #[test]
    fn detects_json_clients() {
        let mut h = HeaderMap::new();
        assert!(!wants_json(&h));
        h.insert("x-requested-with", "JSONHttpRequest".parse().unwrap());
        assert!(wants_json(&h));
        let mut h = HeaderMap::new();
        h.insert(header::ACCEPT, "application/json".parse().unwrap());
        assert!(wants_json(&h));
        h.insert(
            header::ACCEPT,
            "text/html,application/xhtml+xml,application/json;q=0.9"
                .parse()
                .unwrap(),
        );
        assert!(!wants_json(&h));
    }
}
