//! PrivateBin JSON API v2 integration tests, driving the router in-process
//! against both storage backends.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use privatebin::config::{Config, StorageBackend};
use privatebin::model::delete_token;
use privatebin::{AppState, purge};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const SALT: &str = "integration-test-salt";
const PEER: &str = "203.0.113.7:50000";

struct Harness {
    app: Router,
    state: AppState,
    clock: Arc<AtomicI64>,
    _dir: TempDir,
}

async fn harness(backend: StorageBackend, tweak: impl FnOnce(&mut Config)) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.server.salt = Some(SALT.into());
    config.traffic.limit = 0;
    config.storage.backend = backend;
    config.storage.sqlite_path = dir.path().join("db/privatebin.db");
    config.storage.filesystem_dir = dir.path().join("pastes");
    tweak(&mut config);

    let clock = Arc::new(AtomicI64::new(1_700_000_000));
    let state = privatebin::build_state(config).await.unwrap().with_clock({
        let clock = clock.clone();
        Arc::new(move || clock.load(Ordering::SeqCst))
    });
    Harness {
        app: privatebin::router(state.clone()),
        state,
        clock,
        _dir: dir,
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("JSON body")
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("")
    }
}

impl Harness {
    async fn send(&self, req: Request<Body>) -> Reply {
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn post_from(&self, peer: &str, xff: Option<&str>, body: impl Into<Body>) -> Value {
        let mut req = Request::post("/")
            .header("x-requested-with", "JSONHttpRequest")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(xff) = xff {
            req = req.header("x-forwarded-for", xff);
        }
        let mut req = req.body(body.into()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        let reply = self.send(req).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.header("content-type"), "application/json");
        reply.json()
    }

    async fn post(&self, body: &Value) -> Value {
        self.post_from(PEER, None, body.to_string()).await
    }

    async fn get_json(&self, query: &str) -> Value {
        let req = Request::get(format!("/?{query}"))
            .header("x-requested-with", "JSONHttpRequest")
            .body(Body::empty())
            .unwrap();
        let reply = self.send(req).await;
        assert_eq!(reply.status, StatusCode::OK);
        reply.json()
    }

    async fn get(&self, uri: &str) -> Reply {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    fn advance(&self, secs: i64) {
        self.clock.fetch_add(secs, Ordering::SeqCst);
    }

    async fn create(&self, body: &Value) -> (String, String) {
        let res = self.post(body).await;
        assert_eq!(res["status"], 0, "create failed: {res}");
        (
            res["id"].as_str().unwrap().to_owned(),
            res["deletetoken"].as_str().unwrap().to_owned(),
        )
    }
}

fn fixture() -> Value {
    let raw = include_str!("fixtures/paste_v2.json");
    serde_json::from_str::<Value>(raw).unwrap()["body"].clone()
}

fn paste(formatter: &str, discussion: u8, burn: u8, expire: &str) -> Value {
    json!({
        "v": 2,
        "adata": [
            ["Vp3ZsxwpybbSyzRcEP8kHw==", "w2NsFhOrA0s=", 100000, 256, 128, "aes", "gcm", "zlib"],
            formatter, discussion, burn
        ],
        "ct": "q4n9WsqW0dk+6r7Y1A8h2QXk3vGqtZ6M8rE1x7S0yT0=",
        "meta": { "expire": expire }
    })
}

fn comment(paste_id: &str, parent_id: &str, ct: &str) -> Value {
    json!({
        "v": 2,
        "adata": ["Vp3ZsxwpybbSyzRcEP8kHw==", "w2NsFhOrA0s=", 100000, 256, 128, "aes", "gcm", "zlib"],
        "ct": ct,
        "pasteid": paste_id,
        "parentid": parent_id,
    })
}

/// Comment as sent by current PrivateBin clients: the ids are appended to the
/// cipher spec so they are covered by the AEAD associated data.
fn bound_comment(paste_id: &str, parent_id: &str, ct: &str) -> Value {
    let mut body = comment(paste_id, parent_id, ct);
    body["adata"]
        .as_array_mut()
        .unwrap()
        .extend([json!(paste_id), json!(parent_id)]);
    body
}

fn assert_error(res: &Value, message: &str) {
    assert_eq!(res["status"], 1, "expected an error, got {res}");
    assert_eq!(res["message"], message);
}

const NOT_FOUND: &str = "Paste does not exist, has expired or has been deleted.";

async fn create_read_delete(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let body = fixture();
    let res = h.post(&body).await;
    assert_eq!(res["status"], 0);
    let id = res["id"].as_str().unwrap();
    assert_eq!(id.len(), 16);
    assert_eq!(res["url"], format!("/?{id}"));
    assert_eq!(res["deletetoken"], delete_token(SALT.as_bytes(), id));

    let read = h.get_json(id).await;
    assert_eq!(read["status"], 0);
    assert_eq!(read["id"], id);
    assert_eq!(read["url"], format!("/?{id}"));
    assert_eq!(read["v"], 2);
    // adata and ct must round-trip exactly: adata is the AEAD associated data.
    assert_eq!(read["adata"], body["adata"]);
    assert_eq!(
        serde_json::to_string(&read["adata"]).unwrap(),
        serde_json::to_string(&body["adata"]).unwrap()
    );
    assert_eq!(read["ct"], body["ct"]);
    assert_eq!(read["meta"]["created"], 1_700_000_000);
    assert_eq!(read["meta"]["time_to_live"], 604_800);
    assert_eq!(read["comments"], json!([]));
    assert_eq!(read["comment_count"], 0);
    assert_eq!(read["comment_offset"], 0);
    assert_eq!(read["@context"], "?jsonld=paste");

    // `?pasteid=` form reads too.
    assert_eq!(h.get_json(&format!("pasteid={id}")).await["status"], 0);

    let token = res["deletetoken"].as_str().unwrap();
    let del = h.post(&json!({"pasteid": id, "deletetoken": token})).await;
    assert_eq!(
        del,
        json!({"status": 0, "id": id, "url": format!("/?{id}")})
    );
    assert_error(&h.get_json(id).await, NOT_FOUND);
    assert_error(
        &h.post(&json!({"pasteid": id, "deletetoken": token})).await,
        NOT_FOUND,
    );
}

async fn delete_via_get(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (id, token) = h.create(&paste("plaintext", 0, 0, "1day")).await;

    // A browser (no JSON header) gets the web app; nothing is deleted yet.
    let html = h.get(&format!("/?pasteid={id}&deletetoken={token}")).await;
    assert!(html.header("content-type").starts_with("text/html"));
    assert_eq!(h.get_json(&id).await["status"], 0);

    let res = h
        .get_json(&format!("pasteid={id}&deletetoken={token}"))
        .await;
    assert_eq!(res["status"], 0);
    assert_error(&h.get_json(&id).await, NOT_FOUND);
}

async fn wrong_token(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (id, token) = h.create(&paste("plaintext", 0, 0, "1day")).await;
    let wrong = format!("{}0", &token[..token.len() - 1]);
    let wrong = if wrong == token {
        format!("{}1", &token[..63])
    } else {
        wrong
    };
    for bad in [wrong.as_str(), "", "deadbeef"] {
        assert_error(
            &h.post(&json!({"pasteid": id, "deletetoken": bad})).await,
            "Wrong deletion token. Paste was not deleted.",
        );
    }
    assert_eq!(h.get_json(&id).await["status"], 0);
    assert_error(
        &h.post(&json!({"pasteid": "nothex", "deletetoken": token}))
            .await,
        "Invalid paste ID.",
    );
}

async fn burn_after_reading(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (id, _) = h.create(&paste("markdown", 0, 1, "1day")).await;
    let first = h.get_json(&id).await;
    assert_eq!(first["status"], 0);
    assert_eq!(first["adata"][3], 1);
    assert_error(&h.get_json(&id).await, NOT_FOUND);
}

async fn burn_after_reading_concurrent(backend: StorageBackend) {
    let h = Arc::new(harness(backend, |_| {}).await);
    let (id, _) = h.create(&paste("plaintext", 0, 1, "1day")).await;
    let readers: Vec<_> = (0..16)
        .map(|_| {
            let h = h.clone();
            let id = id.clone();
            tokio::spawn(async move { h.get_json(&id).await["status"] == 0 })
        })
        .collect();
    let mut successes = 0;
    for reader in readers {
        if reader.await.unwrap() {
            successes += 1;
        }
    }
    assert_eq!(
        successes, 1,
        "a burn-after-reading paste was read {successes} times"
    );
}

async fn expiry(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (short, _) = h.create(&paste("plaintext", 0, 0, "5min")).await;
    let (never, _) = h.create(&paste("plaintext", 0, 0, "never")).await;
    let (unknown, _) = h.create(&paste("plaintext", 0, 0, "3fortnights")).await;

    h.advance(120);
    let read = h.get_json(&short).await;
    assert_eq!(read["meta"]["time_to_live"], 180);
    let read = h.get_json(&never).await;
    assert!(read["meta"].get("time_to_live").is_none());
    // Unknown expire keys fall back to the default (1 week).
    assert_eq!(
        h.get_json(&unknown).await["meta"]["time_to_live"],
        604_800 - 120
    );

    h.advance(180);
    assert_error(&h.get_json(&short).await, NOT_FOUND);
    assert_eq!(h.get_json(&never).await["status"], 0);

    // The purge removes the remaining expired paste without a read.
    h.advance(604_800);
    assert_eq!(purge::run_once(&h.state).await, 1);
    assert!(h.state.storage.get_paste(&unknown).await.unwrap().is_none());
    assert!(h.state.storage.get_paste(&never).await.unwrap().is_some());
}

async fn comments(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (id, token) = h.create(&paste("plaintext", 1, 0, "1day")).await;

    let top = h.post(&comment(&id, &id, "Y29tbWVudCAx")).await;
    assert_eq!(top["status"], 0, "{top}");
    let top_id = top["id"].as_str().unwrap().to_owned();
    assert_eq!(top["url"], format!("/?{id}#{top_id}"));

    h.advance(5);
    let reply_body = bound_comment(&id, &top_id, "cmVwbHk=");
    let reply = h.post(&reply_body).await;
    assert_eq!(reply["status"], 0, "{reply}");
    let reply_id = reply["id"].as_str().unwrap().to_owned();

    // Ids bound in adata must match the request.
    let mut forged = bound_comment(&id, &top_id, "eA==");
    forged["parentid"] = json!(id);
    assert_error(&h.post(&forged).await, "Invalid data.");

    // Unknown parent, unknown paste, invalid ids.
    assert_error(
        &h.post(&comment(&id, "0123456789abcdef", "eA==")).await,
        "Invalid data.",
    );
    assert_error(
        &h.post(&comment("0123456789abcdef", "0123456789abcdef", "eA=="))
            .await,
        "Invalid data.",
    );
    assert_error(
        &h.post(&comment("zz", &id, "eA==")).await,
        "Invalid paste ID.",
    );

    let read = h.get_json(&id).await;
    assert_eq!(read["comment_count"], 2);
    let list = read["comments"].as_array().unwrap();
    assert_eq!(list[0]["id"], top_id);
    assert_eq!(list[0]["pasteid"], id);
    assert_eq!(list[0]["parentid"], id);
    assert_eq!(list[0]["v"], 2);
    assert_eq!(list[0]["ct"], "Y29tbWVudCAx");
    assert_eq!(list[0]["adata"][0], "Vp3ZsxwpybbSyzRcEP8kHw==");
    assert_eq!(list[0]["meta"]["created"], 1_700_000_000);
    assert_eq!(list[1]["id"], reply_id);
    assert_eq!(list[1]["parentid"], top_id);
    // adata is returned verbatim: it is the AEAD associated data.
    assert_eq!(
        serde_json::to_string(&list[1]["adata"]).unwrap(),
        serde_json::to_string(&reply_body["adata"]).unwrap()
    );
    assert_eq!(list[1]["adata"].as_array().unwrap().len(), 10);
    assert_eq!(list[1]["meta"]["created"], 1_700_000_005);

    // Deleting the paste removes its comments.
    assert_eq!(
        h.post(&json!({"pasteid": id, "deletetoken": token})).await["status"],
        0
    );
    assert!(!h.state.storage.comment_exists(&id, &top_id).await.unwrap());
}

async fn comments_rejected(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let (closed, _) = h.create(&paste("plaintext", 0, 0, "1day")).await;
    assert_error(
        &h.post(&comment(&closed, &closed, "eA==")).await,
        "Invalid data.",
    );
    let (burn, _) = h.create(&paste("plaintext", 0, 1, "1day")).await;
    assert_error(
        &h.post(&comment(&burn, &burn, "eA==")).await,
        "Invalid data.",
    );
    // Burn and discussion together are invalid.
    assert_error(
        &h.post(&paste("plaintext", 1, 1, "1day")).await,
        "Invalid data.",
    );
    // Comments on an expired paste.
    let (open, _) = h.create(&paste("plaintext", 1, 0, "5min")).await;
    h.advance(301);
    assert_error(
        &h.post(&comment(&open, &open, "eA==")).await,
        "Invalid data.",
    );
}

async fn discussion_disabled(backend: StorageBackend) {
    let h = harness(backend, |c| c.main.discussion = false).await;
    assert_error(
        &h.post(&paste("plaintext", 1, 0, "1day")).await,
        "Discussion is disabled.",
    );
    let (id, _) = h.create(&paste("plaintext", 0, 0, "1day")).await;
    assert_error(
        &h.post(&comment(&id, &id, "eA==")).await,
        "Discussion is disabled.",
    );
}

async fn size_limit(backend: StorageBackend) {
    let h = harness(backend, |c| c.main.size_limit = 4096).await;
    let mut body = paste("plaintext", 0, 0, "1day");
    body["ct"] = json!("A".repeat(4096));
    assert_eq!(h.post(&body).await["status"], 0);

    body["ct"] = json!("A".repeat(4100));
    assert_error(
        &h.post(&body).await,
        "Paste is limited to 4 KiB of encrypted data.",
    );
    // Bodies far above the limit are cut off before JSON parsing.
    body["ct"] = json!("A".repeat(200_000));
    assert_error(
        &h.post(&body).await,
        "Paste is limited to 4 KiB of encrypted data.",
    );
}

async fn comment_limits(backend: StorageBackend) {
    let h = harness(backend, |c| {
        c.main.comment_size_limit = 64;
        c.main.max_comments = 2;
    })
    .await;
    let (id, _) = h.create(&paste("plaintext", 1, 0, "1day")).await;
    assert_error(
        &h.post(&comment(&id, &id, &"A".repeat(68))).await,
        "Comment is limited to 64 B of encrypted data.",
    );
    for _ in 0..2 {
        assert_eq!(h.post(&comment(&id, &id, "eA==")).await["status"], 0);
    }
    assert_error(
        &h.post(&comment(&id, &id, "eA==")).await,
        "This discussion is full: no more comments can be added.",
    );
}

async fn hostile_bodies(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    // PBKDF2 iteration counts outside the sane range would stall readers.
    let mut body = paste("plaintext", 0, 0, "1day");
    body["adata"][0][2] = json!(4_000_000_000_u64);
    assert_error(&h.post(&body).await, "Invalid data.");
    // Oversized non-ciphertext fields and non-string ciphertext.
    let mut body = paste("plaintext", 0, 0, "1day");
    body["meta"]["padding"] = json!(vec![0; 5000]);
    assert_error(&h.post(&body).await, "Invalid data.");
    let mut body = paste("plaintext", 0, 0, "1day");
    body["ct"] = json!(vec![0; 100]);
    assert_error(&h.post(&body).await, "Invalid data.");
    // Unknown fields are ignored, whatever their size.
    let mut body = paste("plaintext", 0, 0, "1day");
    body["extra"] = json!(vec![0; 100_000]);
    assert_eq!(h.post(&body).await["status"], 0);
}

async fn invalid_requests(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    for raw in ["", "not json", "[]", "{\"v\":2}"] {
        assert_error(
            &h.post_from(PEER, None, raw.to_owned()).await,
            "Invalid data.",
        );
    }
    assert_error(&h.get_json("0123").await, "Invalid paste ID.");
    assert_error(&h.get_json("0123456789abcdef").await, NOT_FOUND);
}

async fn rate_limit(backend: StorageBackend) {
    let h = harness(backend, |c| c.traffic.limit = 10).await;
    let body = paste("plaintext", 0, 0, "1day").to_string();
    let wait = "Please wait 10 seconds between each post.";

    assert_eq!(h.post_from(PEER, None, body.clone()).await["status"], 0);
    assert_error(&h.post_from(PEER, None, body.clone()).await, wait);
    // Spoofed X-Forwarded-For from an untrusted peer is ignored.
    assert_error(
        &h.post_from(PEER, Some("198.51.100.1"), body.clone()).await,
        wait,
    );
    assert_eq!(
        h.post_from("198.51.100.2:1", None, body.clone()).await["status"],
        0
    );

    // Behind a trusted proxy the forwarded client address is limited.
    let proxy = "127.0.0.1:40000";
    assert_eq!(
        h.post_from(proxy, Some("192.0.2.1"), body.clone()).await["status"],
        0
    );
    assert_error(
        &h.post_from(proxy, Some("192.0.2.1"), body.clone()).await,
        wait,
    );
    assert_eq!(
        h.post_from(proxy, Some("192.0.2.2"), body.clone()).await["status"],
        0
    );
}

async fn html_vs_json(backend: StorageBackend) {
    let h = harness(backend, |c| c.main.name = "Bin <&>".into()).await;
    let (id, _) = h.create(&paste("plaintext", 0, 0, "1day")).await;

    for uri in ["/".to_owned(), format!("/?{id}"), format!("/?pasteid={id}")] {
        let reply = h.get(&uri).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.header("content-type"), "text/html; charset=utf-8");
        assert_eq!(reply.header("cache-control"), "no-store");
        let html = reply.text();
        assert!(!html.contains("<!--PB_CONFIG-->"));
        assert!(
            html.contains(r#""name":"Bin \u003c\u0026\u003e""#),
            "{html}"
        );
        assert!(html.contains(r#""default":"1week""#));
        assert!(reply.header("access-control-allow-origin").is_empty());
    }

    // Accept: application/json also selects the API.
    let req = Request::get(format!("/?{id}"))
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    let reply = h.send(req).await;
    assert_eq!(reply.json()["id"], id);
    assert_eq!(reply.header("access-control-allow-origin"), "*");
    assert_eq!(reply.header("cache-control"), "no-store");

    // Viewing the HTML page does not burn a burn-after-reading paste.
    let (burn, _) = h.create(&paste("plaintext", 0, 1, "1day")).await;
    h.get(&format!("/?{burn}")).await;
    assert_eq!(h.get_json(&burn).await["status"], 0);
}

async fn headers_and_static(backend: StorageBackend) {
    let h = harness(backend, |_| {}).await;
    let reply = h.get("/healthz").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.text(), "ok");
    assert_eq!(
        reply.header("content-security-policy"),
        privatebin::security::CONTENT_SECURITY_POLICY
    );
    assert_eq!(reply.header("referrer-policy"), "no-referrer");
    assert_eq!(reply.header("x-content-type-options"), "nosniff");
    assert_eq!(reply.header("x-frame-options"), "DENY");
    assert_eq!(reply.header("cross-origin-opener-policy"), "same-origin");
    assert_eq!(reply.header("cross-origin-resource-policy"), "same-origin");
    assert!(
        reply
            .header("permissions-policy")
            .contains("interest-cohort=()")
    );

    let missing = h.get("/assets/does-not-exist.js").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(h.get("/nope/deeper").await.status, StatusCode::NOT_FOUND);
    assert_eq!(h.get("/index.html").await.status, StatusCode::NOT_FOUND);

    let preflight = h
        .send(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(preflight.status, StatusCode::OK);
    assert_eq!(preflight.header("access-control-allow-origin"), "*");
    assert_eq!(
        preflight.header("access-control-allow-methods"),
        "GET, POST, PUT, DELETE"
    );
    assert_eq!(
        preflight.header("access-control-allow-headers"),
        "X-Requested-With, Content-Type"
    );
}

/// Served only when the frontend has been built (`web/dist/assets/app.js`).
#[tokio::test]
async fn static_assets_are_cached_when_present() {
    let h = harness(StorageBackend::Sqlite, |_| {}).await;
    let reply = h.get("/assets/app.js").await;
    if reply.status == StatusCode::NOT_FOUND {
        return;
    }
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.header("content-type").contains("javascript"));
    assert_eq!(
        reply.header("cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert!(!reply.header("content-security-policy").is_empty());
}

#[tokio::test]
async fn filesystem_purge_removes_orphaned_discussions() {
    let h = harness(StorageBackend::Filesystem, |_| {}).await;
    let (id, _) = h.create(&paste("plaintext", 1, 0, "never")).await;
    assert_eq!(h.post(&comment(&id, &id, "eA==")).await["status"], 0);
    let root = h.state.config.storage.filesystem_dir.clone();
    let shard = root.join(&id[..2]).join(&id[2..4]);
    let discussion = shard.join(format!("{id}.discussion"));
    assert!(discussion.is_dir(), "unexpected layout under {root:?}");
    // Simulate a comment that raced with the deletion of its paste.
    std::fs::remove_file(shard.join(format!("{id}.json"))).unwrap();
    purge::run_once(&h.state).await;
    assert!(!discussion.exists());
}

#[tokio::test]
async fn in_memory_sqlite_works() {
    use privatebin::storage::{SqliteStorage, Storage};
    let storage = SqliteStorage::in_memory().await.unwrap();
    assert!(
        storage
            .get_paste("0123456789abcdef")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(storage.purge_expired(0).await.unwrap(), 0);
}

#[tokio::test]
async fn jsonld_schemas_for_api_clients() {
    let h = harness(StorageBackend::Sqlite, |_| {}).await;
    // pbincli reads the API version from ?jsonld=paste, without the JSON header.
    let reply = h.get("/?jsonld=paste").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-type"), "application/ld+json");
    let doc = reply.json();
    assert_eq!(doc["@context"]["v"]["@value"], 2);
    assert_eq!(doc["@context"]["pb"], "/?jsonld=types#");

    let types = h.get("/?jsonld=types").await.text();
    assert!(
        types
            .contains(r#"["5min", "10min", "1hour", "1day", "1week", "1month", "1year", "never"]"#)
    );

    assert_eq!(h.get("/?jsonld=../../etc/passwd").await.text(), "{}");
}

macro_rules! backend_tests {
    ($($name:ident),* $(,)?) => {
        mod sqlite {
            $(
                #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
                async fn $name() {
                    super::$name(privatebin::config::StorageBackend::Sqlite).await;
                }
            )*
        }
        mod filesystem {
            $(
                #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
                async fn $name() {
                    super::$name(privatebin::config::StorageBackend::Filesystem).await;
                }
            )*
        }
    };
}

backend_tests!(
    create_read_delete,
    delete_via_get,
    wrong_token,
    burn_after_reading,
    burn_after_reading_concurrent,
    expiry,
    comments,
    comments_rejected,
    discussion_disabled,
    size_limit,
    comment_limits,
    hostile_bodies,
    invalid_requests,
    rate_limit,
    html_vs_json,
    headers_and_static,
);
