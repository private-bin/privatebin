# PrivateBin (Rust)

A self-hosted, zero-knowledge pastebin. Pastes are compressed and encrypted in
the browser (AES-256-GCM, key derived with PBKDF2-SHA256). The key lives only in
the URL fragment after `#`, which browsers never send, so the server stores
nothing but ciphertext.

This is a rewrite of [PrivateBin](https://github.com/PrivateBin/PrivateBin) with
a Rust server and a new web interface. It speaks the PrivateBin **JSON API v2**:
pastes are interchangeable with upstream PrivateBin and clients such as
[pbincli](https://github.com/r4sas/PBinCLI) keep working.

- Client-side encryption, optional password, expiration, burn after reading
- Encrypted, threaded discussions
- Plain text, syntax-highlighted code and Markdown (sanitized)
- Encrypted file attachments with image/video/audio preview
- Delete links, clone, raw view, download, QR code
- Light and dark themes, responsive layout
- SQLite (default) or filesystem storage, automatic purge of expired pastes
- Per-IP rate limiting, strict CSP, request size, time and concurrency limits
- ~15 MB distroless image for `linux/amd64` and `linux/arm64`: one static
  binary, non-root, read-only root filesystem

## Deploy with Docker Compose

Requirements: Docker with the Compose plugin, and a reverse proxy that
terminates **HTTPS**. Browsers only allow the encryption API (WebCrypto) on
secure origins, so the site does not work over plain HTTP except on
`localhost`.

### 1. Get the compose file

```sh
mkdir privatebin && cd privatebin
curl -fsSLO https://raw.githubusercontent.com/private-bin/privatebin/main/docker-compose.yml
```

It runs `ghcr.io/private-bin/privatebin:latest`, publishes the port on
`127.0.0.1:8080` only, and stores data in `./data`.

### 2. Create the data directory

The container runs as uid `65532`, which must be able to write to `./data`:

```sh
mkdir -p data
sudo chown 65532:65532 data   # Linux; not needed with Docker Desktop
```

### 3. Start it

```sh
docker compose up -d
docker compose ps        # STATUS should become "healthy"
```

On first start the server creates `data/privatebin.db` and `data/salt`.

### 4. Put a reverse proxy in front

Point your HTTPS virtual host at `http://127.0.0.1:8080`. A complete Nginx
site is in [`docs/nginx.example.conf`](docs/nginx.example.conf). The parts
that matter:

```nginx
client_max_body_size 16m;      # above main.size_limit (10 MiB) + JSON overhead
access_log off;                # URLs contain paste IDs and delete tokens
add_header Strict-Transport-Security "max-age=63072000; includeSubDomains" always;

location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

The server sets the CSP and the other security headers itself.

## Configuration

Every setting has a default, so the compose file works as is. To change
settings, add environment variables named `PRIVATEBIN__<SECTION>__<KEY>` to
`docker-compose.yml`:

```yaml
    environment:
      PRIVATEBIN__MAIN__NAME: "Team Paste"
      PRIVATEBIN__MAIN__SIZE_LIMIT: 20971520
      PRIVATEBIN__EXPIRE__DEFAULT: 1day
```

or download [`config.example.toml`](config.example.toml) as `config.toml`, edit
it, and uncomment its mount in `docker-compose.yml`. Environment variables take
precedence over the file. Then apply the change with `docker compose up -d`.

| Variable (`PRIVATEBIN__…`) | Default | |
|---|---|---|
| `MAIN__NAME` | `PrivateBin` | Name shown in the interface |
| `MAIN__SIZE_LIMIT` | `10485760` | Max encrypted paste size in bytes (attachments up to ~7.5 MB) |
| `MAIN__DISCUSSION` / `MAIN__PASSWORD` / `MAIN__FILE_UPLOAD` | `true` | Feature toggles |
| `MAIN__BURN_AFTER_READING_SELECTED` | `false` | Pre-select "burn after reading" |
| `MAIN__DEFAULT_FORMATTER` | `plaintext` | `plaintext`, `syntaxhighlighting` or `markdown` |
| `MAIN__COMMENT_SIZE_LIMIT` | `65536` | Max encrypted size of one comment |
| `MAIN__MAX_COMMENTS` | `500` | Max comments per paste |
| `EXPIRE__DEFAULT` | `1week` | Default expiration (the options are in `config.example.toml`) |
| `TRAFFIC__LIMIT` | `10` | Seconds between two posts from one IP (0 = off) |
| `TRAFFIC__TRUSTED_PROXIES` | `127.0.0.1/32,172.16.0.0/12` | Peers whose `X-Forwarded-For` is trusted |
| `STORAGE__BACKEND` | `sqlite` | `sqlite` or `filesystem` |
| `PURGE__INTERVAL_SECS` | `300` | How often expired pastes are deleted (0 = only on access) |
| `SERVER__SALT` | generated | Key for delete tokens; by default generated into `data/salt` |

**Rate limiting behind a proxy.** The defaults trust Docker's bridge networks,
because a proxy on the host reaches the container through the Docker gateway.
Anything that connects from a trusted network can set its own
`X-Forwarded-For` and bypass the rate limit, so never publish the container
port to other hosts, and on shared Docker hosts narrow the list to your compose
network's gateway.

The server also limits what one client can cost: headers must arrive within
30 s, a request must finish within 60 s, at most 16 API requests run at once,
and only the known JSON fields are parsed.

## Operations

**Logs and health**

```sh
docker compose logs -f
docker compose exec privatebin /privatebin healthcheck   # exit code 0 = healthy
```

The server never logs paste IDs, delete tokens or client addresses.

**Upgrade**

```sh
docker compose pull && docker compose up -d
```

`latest` follows every release. To control upgrades, pin a version in
`docker-compose.yml`: `:0.1` gets patch releases only, `:0.1.0` is exact.
Releases are listed on the [releases page](https://github.com/private-bin/privatebin/releases).

**Backup and restore**

`./data` holds everything, and only ciphertext:

- `privatebin.db` with `privatebin.db-wal` and `privatebin.db-shm` (SQLite in
  WAL mode; recent pastes may still be in the `-wal` file), or the paste files
  with the filesystem backend;
- `salt`, the key behind delete links. Without it, existing delete links stop
  working.

Back up the whole directory, preferably while the container is stopped:

```sh
docker compose stop
tar czf privatebin-backup-$(date +%F).tgz data
docker compose start
```

To restore, stop the container, replace `./data` with the backup (keeping the
`65532` owner on Linux) and start it again.

**Purge expired pastes now**

```sh
docker compose exec privatebin /privatebin purge
```

**Troubleshooting**

| Symptom | Cause |
|---|---|
| Page shows "HTTPS required" | The site is served over plain HTTP; put it behind HTTPS |
| Container restarts and the log says `Permission denied` for `/data` | `./data` is not writable by uid 65532 (see step 2) |
| Uploads fail with "413 Request Entity Too Large" | The proxy's body limit is below `MAIN__SIZE_LIMIT` |
| Everyone gets "Please wait 10 seconds" | The proxy's address is not in `TRAFFIC__TRUSTED_PROXIES`, so all users share one IP |
| `bind: address already in use` | Port 8080 is taken; change the left side of `127.0.0.1:8080:8080` |

## API clients

The JSON API is PrivateBin's v2 API. Pastes from pbincli open in the web
interface. Like upstream PrivateBin, the web interface stores attachments as
lists, which some pbincli versions cannot read (the text still decrypts).

## Development

```sh
# frontend (Node 26 LTS): outputs web/dist, which is embedded into the binary
cd web && npm ci && npm run build
npm test                 # crypto compatibility tests
npm run watch            # rebuild on change

# server, with data in ./data instead of /data
export PRIVATEBIN__STORAGE__SQLITE_PATH=./data/privatebin.db PRIVATEBIN__SERVER__SALT_FILE=./data/salt
cargo run -- serve --config config.example.toml
cargo test && cargo clippy --all-targets -- -D warnings

# container image from source, then run it with the compose file
docker build -t ghcr.io/private-bin/privatebin:latest .
docker compose up -d
```

Rebuild the binary after rebuilding the frontend: it embeds `web/dist` at
compile time.

**Browser tests.** The Playwright suite in `e2e/` drives a real browser
against a running server with the rate limit disabled:

```sh
PRIVATEBIN__TRAFFIC__LIMIT=0 cargo run -- serve --config config.example.toml &
cd e2e && npm ci && npx playwright install chromium
PB_URL=http://127.0.0.1:8080/ npx playwright test   # PB_CHANNEL=chrome uses a system Chrome
```

**Dependency checks.** `cargo deny check` (advisories, licenses, sources) and
`cargo audit`. Both run in CI along with the tests and the browser suite.

**Releases.** Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`. It
builds the `linux/amd64` and `linux/arm64` images on native runners,
publishes them to `ghcr.io/private-bin/privatebin` as one multi-arch image with
SBOM and provenance, and creates a GitHub release. The version shown in the
footer and by `privatebin --version` is taken from the tag; other builds show
the version in `Cargo.toml` (override with `--build-arg VERSION=…` or the
`PRIVATEBIN_VERSION` environment variable at build time).

## License

MIT © 2026 INT2. See [LICENSE](LICENSE).

This is an independent rewrite that keeps the API and encryption format of
[PrivateBin](https://github.com/PrivateBin/PrivateBin). The JSON-LD schemas in
`crates/server/jsonld/` are copied from PrivateBin and remain under the zlib
License ([notice](crates/server/jsonld/LICENSE)).
