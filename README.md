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
| `make build-wasm` | Build WASM for Node.js and browser (`dist/node`, `dist/web`) |
| `make build-wasi` | Build WASI binary (`dist/wasi/lpdf.wasm`) |
| `make test-wasm` | Run core Rust unit tests |
| `make test-wasi` | Run WASI crate tests |
| `make build-adapter-node` | Install and build the Node.js adapter |
| `make test-adapter-node` | Build and test the Node.js adapter |
| `make build-adapter-dotnet` | Build the .NET adapter |
| `make test-adapter-dotnet` | Build and test the .NET adapter |
| `make build-adapter-php` | Build the PHP Docker image |
| `make test-adapter-php` | Build and run PHP tests via Docker |
| `make build-adapter-python` | Build the Python Docker image |
| `make test-adapter-python` | Build and run Python tests via Docker |
| `make build-all` | Build everything (WASM + WASI + all adapters) |
| `make test-all` | Run all tests across all adapters |
| `make example-all` | Run all examples |
| `make clean-wasm` | Remove WASM build artifacts (`dist/node`, `dist/web`) |
| `make clean-wasi` | Remove WASI build artifacts (`dist/wasi`) |
| `make clean-adapter-node` | Remove Node.js adapter build artifacts |
| `make clean-adapter-dotnet` | Remove .NET adapter build artifacts |
| `make clean-adapter-php` | Remove PHP Docker image |
| `make clean-adapter-python` | Remove Python Docker image |
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

## VS Code Extension

The extension provides live preview, PDF export, code generation, and XSD-backed XML validation for `.lpdf` files.

| Command | Description |
|---|---|
| `make build-adapter-vscode` | Compile TypeScript, copy WASM + schema |
| `make package-adapter-vscode` | Package into `src/adapters/vscode/lpdf.vsix` |
| `make install-adapter-vscode` | Package and install into VS Code |

To update the extension after making changes (schema, WASM, or TypeScript source):

```sh
make install-adapter-vscode
```

Then **restart VS Code** (or run **Developer: Restart Extension Host** from the Command Palette) to load the new version. The install step automatically rebuilds and repackages before installing.

> **Note:** `build-adapter-vscode` copies `schema/lpdf.xsd` from the project root into the extension. Always edit the canonical schema at `schema/lpdf.xsd`; changes made directly to `src/adapters/vscode/schema/lpdf.xsd` will be overwritten on the next build.

---

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
