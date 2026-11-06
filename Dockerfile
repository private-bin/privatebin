# syntax=docker/dockerfile:1

# --- Frontend -----------------------------------------------------------------
# Platform-independent output: always build on the native platform, even for multi-arch images.
FROM --platform=$BUILDPLATFORM node:26-alpine3.24 AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# --- Rust toolchain + cargo-chef ----------------------------------------------
FROM rust:1-alpine3.24 AS chef
RUN apk add --no-cache musl-dev \
 && cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

# --- Static musl binary embedding web/dist -------------------------------------
FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
# Dependencies only: cached until Cargo.toml / Cargo.lock change.
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /src/web/dist ./web/dist
RUN cargo build --release --locked --bin privatebin \
 && cp target/release/privatebin /privatebin \
 && mkdir -p /data \
 && chown 65532:65532 /data

# --- Runtime --------------------------------------------------------------------
FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=builder /privatebin /privatebin
# Owned by the nonroot user so a fresh named volume mounted here is writable.
COPY --from=builder --chown=65532:65532 /data /data
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/privatebin"]
CMD ["serve"]
