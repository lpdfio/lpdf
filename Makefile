.PHONY: build test clean

build:
	wasm-pack build src/core --target nodejs --out-dir ../../dist/node
	wasm-pack build src/core --target web    --out-dir ../../dist/web

test:
	cargo test --manifest-path src/core/Cargo.toml

clean:
	cargo clean --manifest-path src/core/Cargo.toml
	rm -rf dist/node dist/web
