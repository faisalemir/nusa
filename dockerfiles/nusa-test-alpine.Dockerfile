FROM rust:1.95-alpine
RUN apk add --no-cache musl-dev pkgconfig openssl-dev lld
RUN cargo install cargo-nextest --locked
