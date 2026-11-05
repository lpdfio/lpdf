# The tools scripts/build-ffi.sh needs, for `make build-ffi` on a machine without them: Rust with the two Linux
# targets, cargo-zigbuild with zig, and objdump. zig and cargo-zigbuild are pinned to the versions core's CI installs.
FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends python3-venv binutils && \
    python3 -m venv /opt/zig && \
    /opt/zig/bin/pip install --no-cache-dir ziglang==0.17.0 cargo-zigbuild==0.24.0 && \
    rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

ENV PATH=/opt/zig/bin:$PATH
