//! JSON responses in PrivateBin's format, with its CORS headers.

use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::error::ApiError;

pub fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("X-Requested-With, Content-Type"),
    );
    response
}

/// Errors are reported with HTTP 200 and `status: 1`, like PrivateBin.
pub fn into_response(result: Result<Value, ApiError>) -> Response {
    let body = result.unwrap_or_else(|e| json!({ "status": 1, "message": e.message() }));
    with_cors(axum::Json(body).into_response())
}

pub fn paste_url(id: &str) -> String {
    format!("/?{id}")
}
