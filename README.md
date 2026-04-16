# lpdf

A declarative XML language for defining PDF layouts. A core layout engine compiles XML into a resolved render tree (absolute positions, sizes); thin adapters translate that tree into calls on whatever PDF library the consumer already uses.

See [var/LPDF.md](var/LPDF.md) for full design notes.

## Build

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) and a Rust toolchain with the `wasm32-unknown-unknown` and `wasm32-wasip1` targets:

```sh
rustup target add wasm32-unknown-unknown
rustup target add wasm32-wasip1
```

| Command | Description |
|---|---|
| `make build` | Build WASM for Node.js and browser (`dist/node`, `dist/web`) |
| `make wasi` | Build WASI binary (`dist/wasi/lpdf.wasm`) |
| `make test` | Run core Rust unit tests |
| `make test-wasi` | Run WASI crate tests |
| `make adapter-node` | Install and build the Node.js adapter |
| `make adapter-node-test` | Build and test the Node.js adapter |
| `make adapter-dotnet` | Build the .NET adapter (requires `wasi` first) |
| `make adapter-dotnet-test` | Build and test the .NET adapter |
| `make adapter-php` | Build the PHP Docker image (requires `wasi` first) |
| `make adapter-php-test` | Build and run PHP tests via Docker |
| `make build-all` | Build everything (WASM + WASI + all adapters) |
| `make test-all` | Run all tests across all adapters |
| `make clean` | Remove all build artifacts |

## Examples

Each example reads `invoice.xml` from the project root and writes the PDF to `example/`. Run `make adapter-<name>` first to ensure the adapter is built.

### Node.js

```sh
make example-node
# Output: example/invoice-node.pdf
```

### .NET

```sh
make example-dotnet
# Output: example/invoice-dotnet.pdf
```

### PHP

```sh
make example-php
# Output: example/invoice-php.pdf
```

## Verify

After building, run the smoke tests against the compiled WASM:

```sh
node test/verify-wasm.mjs        # Node.js
# open test/verify-wasm.html     # browser (requires a local HTTP server)
```

## Structure

```
src/core/            Rust crate — layout engine, compiled to .wasm
src/core-wasi/       Rust crate — WASI CLI wrapper

src/adapters/node/   Node.js / browser adapter (TypeScript)
src/adapters/dotnet/ .NET adapter (C#)
src/adapters/php/    PHP adapter (runs WASM via Docker)

dist/node/           wasm-pack output for Node.js (generated)
dist/web/            wasm-pack output for browsers (generated)
dist/wasi/           WASI binary (generated)

test/                smoke tests and snapshot fixtures

schema/              lpdf XML schema (XSD)

example/             output folder for generated example PDFs
var/                 design documents and architecture notes
```
