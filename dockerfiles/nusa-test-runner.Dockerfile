FROM rust:1.95-alpine
RUN apk add --no-cache musl-dev git pkgconfig openssl-dev mold clang-dev

RUN cargo install cargo-nextest --locked

WORKDIR /src

# Layer 1: Cache deps (only rebuilds when Cargo.toml changes)
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY benches/ benches/
RUN rm -f .cargo/config.toml
RUN cargo fetch
RUN cargo build --workspace --lib 2>&1 || true

# Layer 2: Full source + tests
COPY . .
RUN rm -f .cargo/config.toml

CMD ["cargo", "nextest", "run", "--workspace", "--exclude", "nusa-engine-ffi", "--test-threads", "4"]
