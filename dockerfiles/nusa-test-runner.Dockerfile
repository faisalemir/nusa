FROM rust:1.95-alpine
# openssl-libs-static: link gateway/redis (native-tls) tests on musl
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static mold clang-dev

RUN cargo install cargo-nextest cargo-chef --locked

# Prebuilt artifacts live here (not under /src — host mount replaces /src at runtime).
ENV CARGO_TARGET_DIR=/opt/nusa-target
ENV CARGO_INCREMENTAL=1

WORKDIR /src

# Stable musl flags (do not bake host .cargo/config.toml into the image)
COPY .cargo/config-alpine.toml .cargo/config.toml

# Layer 1: dependency cache — rebuilds only when manifests or Cargo.lock change
COPY Cargo.toml Cargo.lock ./
COPY crates/nusa-core/Cargo.toml crates/nusa-core/
COPY crates/nusa-ipc/Cargo.toml crates/nusa-ipc/
COPY crates/nusa-gateway/Cargo.toml crates/nusa-gateway/
COPY crates/nusa-engine-ffi/Cargo.toml crates/nusa-engine-ffi/
COPY crates/nusa-engine-wasm/Cargo.toml crates/nusa-engine-wasm/
COPY crates/nusa-engine-child/Cargo.toml crates/nusa-engine-child/
COPY crates/nusa-plugin-api/Cargo.toml crates/nusa-plugin-api/
COPY crates/nusa-security/Cargo.toml crates/nusa-security/
COPY crates/nusa-config/Cargo.toml crates/nusa-config/
COPY crates/nusa-telemetry/Cargo.toml crates/nusa-telemetry/
COPY crates/nusa-cli/Cargo.toml crates/nusa-cli/
COPY crates/nusa-octane-worker/Cargo.toml crates/nusa-octane-worker/
COPY benches/Cargo.toml benches/

RUN for d in crates/*/ benches; do \
        mkdir -p "${d}/src" && printf '%s\n' '// dependency-layer stub' > "${d}/src/lib.rs"; \
    done \
    && printf 'fn main() {}\n' > crates/nusa-cli/src/main.rs \
    && cargo chef prepare --recipe-path recipe.json \
    && cargo chef cook --recipe-path recipe.json

# Layer 2: full source + precompile all test binaries (used by podman-test-* at runtime)
COPY . .
RUN rm -f .cargo/config.toml.bak \
    && test -f .cargo/config-alpine.toml \
    && cp -f .cargo/config-alpine.toml .cargo/config.toml \
    && cargo nextest run --workspace --test-threads 4 --no-run

CMD ["cargo", "nextest", "run", "--workspace", "--exclude", "nusa-engine-ffi", "--test-threads", "4"]
