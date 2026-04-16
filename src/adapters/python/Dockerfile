FROM python:3.12-slim

RUN apt-get update && apt-get install -y curl xz-utils && \
    curl -sSf https://wasmtime.dev/install.sh | bash && \
    mv /root/.wasmtime/bin/wasmtime /usr/local/bin/wasmtime && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir pytest

WORKDIR /app

COPY lpdf/ ./lpdf/
COPY tests/ ./tests/
COPY resources/ ./resources/
