//! JSON-LD schema documents served on `GET /?jsonld=<type>`, as PrivateBin does.
//! Clients such as pbincli read `?jsonld=paste` to detect the API version.

use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

const PASTE: &str = include_str!("../../jsonld/paste.jsonld");
const PASTE_META: &str = include_str!("../../jsonld/pastemeta.jsonld");
const COMMENT: &str = include_str!("../../jsonld/comment.jsonld");
const COMMENT_META: &str = include_str!("../../jsonld/commentmeta.jsonld");
const TYPES: &str = include_str!("../../jsonld/types.jsonld");

/// The expire options listed in the upstream `types.jsonld`.
const DEFAULT_EXPIRE_ENUM: &str =
    r#""5min", "10min", "1hour", "1day", "1week", "1month", "1year", "never""#;

pub fn jsonld(state: &AppState, kind: &str) -> Response {
    let document = match kind {
        "paste" => PASTE.to_owned(),
        "pastemeta" => PASTE_META.to_owned(),
        "comment" => COMMENT.to_owned(),
        "commentmeta" => COMMENT_META.to_owned(),
        "types" => {
            let options: Vec<String> = state
                .config
                .expire
                .ordered()
                .into_iter()
                .map(|(key, _)| serde_json::Value::from(key.to_string()).to_string())
                .collect();
            TYPES.replace(DEFAULT_EXPIRE_ENUM, &options.join(", "))
        }
        _ => "{}".to_owned(),
    };
    (
        [(header::CONTENT_TYPE, "application/ld+json")],
        document.replace("?jsonld=", "/?jsonld="),
    )
        .into_response()
}
