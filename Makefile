SHELL := C:/PROGRA~1/Git/bin/sh.exe

.PHONY: build test clean adapter-node adapter-node-test adapter-dotnet adapter-dotnet-test build-all test-all

build:
	wasm-pack build src/core --target nodejs --out-dir ../../dist/node
	wasm-pack build src/core --target web    --out-dir ../../dist/web

wasi:
	cargo build --manifest-path src/core-wasi/Cargo.toml --release --target wasm32-wasip1
	mkdir -p dist/wasi && cp target/wasm32-wasip1/release/lpdf-wasi.wasm dist/wasi/lpdf.wasm

test:
	cargo test --manifest-path src/core/Cargo.toml

test-wasi:
	cargo test --manifest-path src/core-wasi/Cargo.toml

adapter-node:
	cd src/adapters/node && npm install && npm run build

adapter-node-test: adapter-node
	cd src/adapters/node && npm test

adapter-dotnet: wasi
	mkdir -p src/adapters/dotnet/wasm && cp dist/wasi/lpdf.wasm src/adapters/dotnet/wasm/lpdf.wasm
	cd src/adapters/dotnet && dotnet build Lpdf.csproj -c Release

adapter-dotnet-test: adapter-dotnet
	cd src/adapters/dotnet && dotnet test

build-all: build wasi adapter-node adapter-dotnet

test-all: test test-wasi adapter-node-test adapter-dotnet-test

clean:
	cargo clean --manifest-path src/core/Cargo.toml
	cargo clean --manifest-path src/core-wasi/Cargo.toml
	rm -rf dist/node dist/web dist/wasi
