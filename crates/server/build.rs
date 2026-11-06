//! Ensures `web/dist/index.html` exists so the crate builds (and embeds a
//! stub page) without running the Node.js frontend build first.

use std::path::Path;

fn main() {
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
