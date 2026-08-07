# Test PHP 8.5 compiled + composer install
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
    && cd / && rm -rf "php-${PHP_VERSION}.tar.gz" "php-${PHP_VERSION}" \
    && apk del build-base autoconf automake libtool re2c bison flex

FROM alpine:3.23
COPY --from=php-build /usr/local/php /usr/local/php
RUN apk add --no-cache composer curl \
    && ln -sf /usr/local/php/bin/php /usr/bin/php \
    && printf '#!/bin/sh\nexec /usr/local/php/bin/php /usr/bin/composer.phar "$@"\n' > /usr/bin/composer \
    && chmod +x /usr/bin/composer

RUN php -v && echo "---" && composer --version
