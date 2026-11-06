//! Security headers applied to every response.

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;

pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
    style-src 'self'; font-src 'self'; img-src 'self' data: blob:; media-src blob:; connect-src 'self'; \
    form-action 'none'; frame-ancestors 'none'; base-uri 'none'";

pub const PERMISSIONS_POLICY: &str =
    "interest-cohort=(), camera=(), microphone=(), geolocation=(), payment=(), usb=()";

const HEADERS: [(HeaderName, &str); 7] = [
    (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    ),
    (
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    ),
    (
        HeaderName::from_static("permissions-policy"),
        PERMISSIONS_POLICY,
    ),
];

/// Adds the security headers to every response, and `Cache-Control: no-store`
/// unless a handler chose a caching policy (static assets do).
pub fn apply<S: Clone + Send + Sync + 'static>(mut router: Router<S>) -> Router<S> {
    for (name, value) in HEADERS {
        router = router.layer(SetResponseHeaderLayer::overriding(
            name,
            HeaderValue::from_static(value),
        ));
    }
    router.layer(SetResponseHeaderLayer::if_not_present(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    ))
}
