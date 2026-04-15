SHELL := C:/Program Files/Git/bin/sh.exe

.PHONY: build test clean adapter-node adapter-node-test

build:
	wasm-pack build src/core --target nodejs --out-dir ../../dist/node
	wasm-pack build src/core --target web    --out-dir ../../dist/web

test:
	cargo test --manifest-path src/core/Cargo.toml

adapter-node:
	cd src/adapters/node && npm install && npm run build

adapter-node-test: adapter-node
	cd src/adapters/node && npm test

clean:
	cargo clean --manifest-path src/core/Cargo.toml
	rm -rf dist/node dist/web
