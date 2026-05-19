# syntax=docker/dockerfile:1

# === Stage 1: Build PHP ZTS ===
FROM alpine:3.19 AS php-builder
RUN apk add --no-cache \
    gcc g++ make autoconf automake libtool pkgconfig \
    libxml2-dev sqlite-dev zlib-dev libpng-dev \
    oniguruma-dev libzip-dev curl-dev \
    linux-headers

WORKDIR /src
RUN curl -fsSL https://www.php.net/distributions/php-8.3.6.tar.gz -o php.tar.gz \
    && tar xzf php.tar.gz --strip-components=1 \
    && ./configure \
        --prefix=/usr/local/php \
        --enable-embed=static \
        --enable-maintainer-zts \
        --enable-fpm \
        --with-zlib \
        --with-curl \
        --with-mysqli \
        --with-pdo-mysql \
        --with-pdo-sqlite \
        --enable-mbstring \
        --enable-intl \
        --enable-opcache \
        --disable-debug \
    && make -j$(nproc) \
    && make install

# === Stage 2: Build Rust binary ===
FROM rust:1.95-alpine AS rust-builder
RUN apk add --no-cache musl-dev pkgconfig git

WORKDIR /src
COPY . .

# Build static binary
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --target x86_64-unknown-linux-musl --bin phprt

# === Stage 3: Runtime ===
FROM alpine:3.19 AS runtime

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates tzdata \
    php8-cli php8-json php8-mbstring php8-pdo php8-sqlite3 \
    && addgroup -g 1000 phprt \
    && adduser -u 1000 -G phprt -s /bin/sh -D phprt \
    && mkdir -p /app/public /tmp/phprt /app/.octane \
    && chown -R phprt:phprt /app /tmp/phprt

WORKDIR /app

# Copy Rust binary
COPY --from=rust-builder /src/target/x86_64-unknown-linux-musl/release/phprt /bin/phprt
COPY --chmod=0644 config.toml.example /app/config.toml

# Copy PHP driver
COPY --from=php-builder /usr/local/php /usr/local/php
ENV PATH="/usr/local/php/bin:$PATH"

COPY php-driver/ /app/php-driver/

RUN chmod +x /bin/phprt

VOLUME ["/app/public", "/tmp/phprt"]

ENV RUST_LOG=info \
    PHPRT_VFS_ROOT=/app/public \
    PHPRT_TMP_DIR=/tmp/phprt

EXPOSE 8080 9090
USER phprt

ENTRYPOINT ["/bin/phprt"]
CMD ["--config", "/app/config.toml"]

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -qO- http://localhost:8080/health || exit 1
