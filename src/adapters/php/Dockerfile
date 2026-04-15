FROM php:cli

RUN apt-get update && apt-get install -y curl xz-utils unzip && \
    curl -sSf https://wasmtime.dev/install.sh | bash && \
    mv /root/.wasmtime/bin/wasmtime /usr/local/bin/wasmtime && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

RUN curl -sS https://getcomposer.org/installer | php -- --install-dir=/usr/local/bin --filename=composer

WORKDIR /app

COPY composer.json composer.lock ./
RUN composer install --no-interaction

COPY src/ ./src/
COPY test/ ./test/
COPY resources/ ./resources/
