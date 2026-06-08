# Multi-stage build for Awaria Daemon
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig git glib-dev gtk+3.0-dev webkit2gtk-4.1-dev libayatana-appindicator-dev

WORKDIR /usr/src/awaria

# Copy source files
COPY src-tauri ./src-tauri
COPY public ./public

WORKDIR /usr/src/awaria/src-tauri

# Build the daemon binary in release mode
# Note: we use musl target to have a fully static binary
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/awaria/src-tauri/target \
    RUSTFLAGS="-C target-feature=-crt-static" cargo build --release --bin awaria-daemon && \
    cp /usr/src/awaria/src-tauri/target/release/awaria-daemon /tmp/awaria-daemon

# Final minimal runner stage
FROM alpine:3.19

RUN apk add --no-cache ca-certificates tzdata glib gtk+3.0 webkit2gtk-4.1 libayatana-appindicator

# Copy compiled binary from builder
COPY --from=builder /tmp/awaria-daemon /usr/local/bin/awaria-daemon

EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/awaria-daemon"]
