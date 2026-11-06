//! Ensures `web/dist/index.html` exists so the crate builds (and embeds a
//! stub page) without running the Node.js frontend build first, and sets the
//! version shown in the UI and `--version`.

use std::path::Path;

fn main() {
    // Release builds pass the git tag (`v1.2.3` or `1.2.3`); other builds use
    // the Cargo.toml version.
    println!("cargo:rerun-if-env-changed=PRIVATEBIN_VERSION");
    let version = std::env::var("PRIVATEBIN_VERSION")
        .ok()
        .map(|v| v.trim().trim_start_matches('v').to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    assert!(
        version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)),
        "PRIVATEBIN_VERSION must look like a version, got {version:?}"
    );
    println!("cargo:rustc-env=PRIVATEBIN_VERSION={version}");

    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    let index = dist.join("index.html");
    if !index.exists() {
        std::fs::create_dir_all(&dist).expect("create web/dist");
        std::fs::write(
            &index,
            "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\">\
             <title>PrivateBin</title>\n\
             <script type=\"application/json\" id=\"pb-config\"><!--PB_CONFIG--></script>\n\
             </head>\n<body><p>Frontend not built: run <code>npm ci &amp;&amp; npm run build</code> \
             in <code>web/</code>.</p></body>\n</html>\n",
        )
        .expect("write placeholder web/dist/index.html");
    }
    println!("cargo:rerun-if-changed=../../web/dist");
    println!("cargo:rerun-if-changed=migrations");
}
