SHELL := C:/PROGRA~1/Git/bin/sh.exe

.PHONY: build test clean adapter-node adapter-node-test adapter-dotnet adapter-dotnet-test adapter-php adapter-php-test build-all test-all example-node example-dotnet example-php

build:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building WASM (node + web)..."
	@echo ""
	wasm-pack build src/core --target nodejs --out-dir ../../dist/node
	wasm-pack build src/core --target web    --out-dir ../../dist/web

wasi:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building WASI binary..."
	@echo ""
	cargo build --manifest-path src/core-wasi/Cargo.toml --release --target wasm32-wasip1
	mkdir -p dist/wasi && cp target/wasm32-wasip1/release/lpdf-wasi.wasm dist/wasi/lpdf.wasm

test:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running core tests..."
	@echo ""
	cargo test --manifest-path src/core/Cargo.toml

test-wasi:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running WASI tests..."
	@echo ""
	cargo test --manifest-path src/core-wasi/Cargo.toml

adapter-node:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Node adapter..."
	@echo ""
	cd src/adapters/node && npm install && npm run build

adapter-node-test: adapter-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Node adapter..."
	@echo ""
	cd src/adapters/node && npm test

adapter-dotnet: wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building .NET adapter..."
	@echo ""
	mkdir -p src/adapters/dotnet/wasm && cp dist/wasi/lpdf.wasm src/adapters/dotnet/wasm/lpdf.wasm
	cd src/adapters/dotnet && dotnet build Lpdf.csproj -c Release

adapter-dotnet-test: adapter-dotnet
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing .NET adapter..."
	@echo ""
	cd src/adapters/dotnet && dotnet test

adapter-php: wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building PHP adapter..."
	@echo ""
	mkdir -p src/adapters/php/resources && cp dist/wasi/lpdf.wasm src/adapters/php/resources/lpdf-wasi.wasm
	docker build -t lpdf-php src/adapters/php

adapter-php-test: adapter-php
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing PHP adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/src://app/src" \
		-v "$(CURDIR)/src/adapters/php/test://app/test" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		-v "$(CURDIR)/src/adapters/php/resources://app/resources" \
		-w //app lpdf-php php vendor/bin/phpunit test

build-all: build wasi adapter-node adapter-dotnet adapter-php

test-all: test test-wasi adapter-node-test adapter-dotnet-test adapter-php-test

example-node: adapter-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running Node example..."
	@echo ""
	cd src/adapters/node && npx ts-node example/example.ts

example-dotnet: adapter-dotnet
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running .NET example..."
	@echo ""
	dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj

example-php: adapter-php
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running PHP example..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-w //app lpdf-php php src/adapters/php/example/example.php

clean:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning build artifacts..."
	@echo ""
	cargo clean --manifest-path src/core/Cargo.toml
	cargo clean --manifest-path src/core-wasi/Cargo.toml
	rm -rf dist/node dist/web dist/wasi
