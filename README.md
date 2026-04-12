# lpdf

A declarative XML language for defining PDF layouts. A core layout engine compiles XML into a resolved render tree (absolute positions, sizes); thin adapters translate that tree into calls on whatever PDF library the consumer already uses.

See [var/LPDF.md](var/LPDF.md) for full design notes.

## Build

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) and a Rust toolchain with the `wasm32-unknown-unknown` target:

```sh
rustup target add wasm32-unknown-unknown
```

**Windows** — `make` is not available by default; use the PowerShell script:

```powershell
.\build-wasm.ps1        # compiles dist/node (Node.js) and dist/web (browser)
```

**Linux / macOS**

```sh
make build    # compiles dist/node and dist/web
make test     # runs Rust unit tests
make clean    # removes build artifacts
```

## Verify

After building, run the smoke tests against the compiled WASM:

```sh
node test/verify-wasm.mjs        # Node.js
# open test/verify-wasm.html     # browser (requires a local HTTP server)
```

## Structure

```
src/core/        Rust crate — layout engine, compiled to .wasm
dist/node/       wasm-pack output for Node.js (gitignored, generated)
dist/web/        wasm-pack output for browsers (gitignored, generated)
test/            smoke tests against the compiled WASM
var/             design documents and architecture notes
```
