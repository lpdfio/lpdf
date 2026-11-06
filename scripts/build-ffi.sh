#!/bin/sh
# Builds the native library (src/core-ffi) for Linux with glibc, on x86_64 and aarch64, into <out>:
#   liblpdf-linux-x86_64.so
#   liblpdf-linux-aarch64.so
# The PHP SDK loads them through FFI. They are built for glibc 2.17 with cargo-zigbuild, so they load on every
# supported Ubuntu and Debian, and the build fails when either needs a newer GLIBC_ symbol than that, as `objdump -T`
# shows it. If Rust or a dependency comes to need more, the baseline is raised here, and in the PHP SDK's docs.
#
# Run from the root of the checkout. Needs the Rust targets x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu,
# cargo-zigbuild with zig, objdump, and LPDF_PUBLIC_KEY. Core's CI and release run it on Ubuntu; `make build-ffi`
# runs it in a container.
set -eu

out=${1:?usage: sh scripts/build-ffi.sh <out-dir>}
glibc=2.17

mkdir -p "$out"
for arch in x86_64 aarch64; do
    target="$arch-unknown-linux-gnu"
    # Only the exported functions need names, and the dynamic symbol table keeps them.
    CARGO_PROFILE_RELEASE_STRIP=symbols \
        cargo zigbuild --manifest-path src/core-ffi/Cargo.toml --release --locked --target "$target.$glibc"
    cp "${CARGO_TARGET_DIR:-target}/$target/release/liblpdf_ffi.so" "$out/liblpdf-linux-$arch.so"
done

failed=0
for lib in "$out/liblpdf-linux-x86_64.so" "$out/liblpdf-linux-aarch64.so"; do
    newest=$(objdump -T "$lib" | grep -o 'GLIBC_[0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n 1)
    for name in lpdf_invoke lpdf_free; do
        if ! objdump -T "$lib" | grep -qw "$name"; then
            echo "::error::$(basename "$lib") does not export $name"
            failed=1
        fi
    done
    echo "$(basename "$lib"): $(wc -c < "$lib" | tr -d ' ') bytes, needs glibc $newest"
    if [ "$(printf '%s\n%s\n' "$newest" "$glibc" | sort -V | tail -n 1)" != "$glibc" ]; then
        echo "::error::$(basename "$lib") needs GLIBC_$newest, newer than the $glibc it is built for. Find what asks for it, or raise the baseline here and in the PHP SDK's docs."
        failed=1
    fi
done
exit "$failed"
