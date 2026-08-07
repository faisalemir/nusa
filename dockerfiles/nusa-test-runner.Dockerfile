# Stage 1: Compile PHP 8.5 from source on Alpine 3.23 (musl)
FROM alpine:3.23 AS php-build
ENV PHP_VERSION=8.5.6
RUN apk add --no-cache \
    build-base autoconf automake libtool re2c bison flex \
    pkgconfig \
    libxml2-dev openssl-dev curl-dev sqlite-dev oniguruma-dev \
    zlib-dev icu-dev libedit-dev argon2-dev \
    && wget -q "https://www.php.net/distributions/php-${PHP_VERSION}.tar.gz" \
    && tar xf "php-${PHP_VERSION}.tar.gz" \
    && cd "php-${PHP_VERSION}" \
    && ./configure \
        --prefix=/usr/local/php \
        --enable-embed \
        --enable-maintainer-zts \
        --enable-cli \
        --enable-fpm \
        --enable-opcache \
        --enable-mbstring \
        --enable-pdo \
        --enable-phar \
        --enable-tokenizer \
        --enable-fileinfo \
        --enable-session \
        --enable-dom \
        --enable-xml \
        --enable-simplexml \
        --enable-xmlreader \
        --enable-xmlwriter \
        --with-curl \
        --with-openssl \
        --with-sqlite3 \
        --with-pdo-sqlite \
        --with-zlib \
        --with-mhash \
        --with-libedit \
        --with-password-argon2 \
        --with-intl \
        --without-pear \
    && make -j$(nproc) \
    && make install \
    && strip /usr/local/php/bin/php \
    && strip /usr/local/php/lib/libphp.so \
    && cd / && rm -rf "php-${PHP_VERSION}.tar.gz" "php-${PHP_VERSION}" \
    && apk del build-base autoconf automake libtool re2c bison flex

# Stage 2: wrk + curl (Alpine 3.23 stable)
FROM alpine:3.23 AS php-tools
RUN apk add --no-cache curl wrk

# Stage 3: Rust + PHP 8.5 (compiled from source)
FROM rust:1.95-alpine
# PHP 8.5 runtime libraries (compiled from source in stage 1)
COPY --from=php-build /usr/local/php /usr/local/php
# Runtime libs that PHP was linked against dynamically
RUN apk add --no-cache \
    libxml2 oniguruma icu-libs libedit libcurl openssl sqlite-libs \
    argon2-libs zlib
# Download composer.phar directly (no Alpine package dependency)
RUN curl -sS https://getcomposer.org/installer | /usr/local/php/bin/php -- --install-dir=/usr/bin --filename=composer.phar \
    && printf '#!/bin/sh\nexec /usr/local/php/bin/php /usr/bin/composer.phar "$@"\n' > /usr/bin/composer \
    && chmod +x /usr/bin/composer
# wrk
COPY --from=php-tools /usr/bin/wrk /usr/bin/
# Symlink + verify (debug: separate commands to find exact failure)
RUN ln -sf /usr/local/php/bin/php /usr/bin/php \
    && ln -sf /usr/local/php/bin/phpize /usr/bin/phpize \
    && ln -sf /usr/local/php/bin/php-config /usr/bin/php-config \
    && ln -sf /usr/local/php/lib/libphp.so /usr/lib/libphp.so \
    && mkdir -p /usr/local/php/lib/php/extensions \
    && ls -la /usr/local/php/bin/ \
    && echo "=== Testing PHP binary ===" \
    && php -v 2>&1; echo "Exit code: $?"

# Build deps
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
COPY crates/nusa-e2e-tests/Cargo.toml crates/nusa-e2e-tests/
COPY crates/nusa-engine-embed/Cargo.toml crates/nusa-engine-embed/
COPY benches/Cargo.toml benches/

RUN for d in crates/*/ benches; do \
        mkdir -p "${d}/src" && printf '%s\n' '// dependency-layer stub' > "${d}/src/lib.rs"; \
    done \
    && printf 'fn main() {}\n' > crates/nusa-cli/src/main.rs \
    && cargo chef prepare --recipe-path recipe.json \
    && cargo chef cook --recipe-path recipe.json

# Layer 2: full source + precompile all test binaries (used by podman-test-* at runtime)
COPY . .

# P2: PHP + Laravel minimal fixture (vendor + php-driver symlink)
RUN cd /src/tests/fixtures/laravel-minimal \
    && ln -sfn /src/php-driver ./php-driver \
    && cp -f .env.example .env \
    && mkdir -p storage/framework/{cache,sessions,views} storage/logs bootstrap/cache database \
    && touch database/database.sqlite \
    && chmod -R a+rwX storage bootstrap/cache database \
    && COMPOSER_ALLOW_SUPERUSER=1 php /usr/bin/composer install --no-interaction --prefer-dist --no-progress \
    && test -f vendor/autoload.php \
    && test -f php-driver/bin/nusa-octane-worker \
    && mkdir -p /usr/local/php/etc/conf.d \
    && cp /src/dockerfiles/php85/99-nusa-opcache.ini /usr/local/php/etc/conf.d/99-nusa-opcache.ini \
    && chmod +x /src/dockerfiles/warm-php-opcache.sh \
    && NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal /src/dockerfiles/warm-php-opcache.sh

ENV NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal

WORKDIR /src
RUN rm -f .cargo/config.toml.bak \
    && test -f .cargo/config-alpine.toml \
    && cp -f .cargo/config-alpine.toml .cargo/config.toml \
    && cargo nextest run --workspace --test-threads 4 --no-run

CMD ["cargo", "nextest", "run", "--workspace", "--exclude", "nusa-engine-ffi", "--test-threads", "4"]
