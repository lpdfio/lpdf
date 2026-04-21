SHELL := C:/PROGRA~1/Git/bin/sh.exe
.SHELLFLAGS := -c

.PHONY: build-wasm build-wasi test-wasm test-wasi \
        build-adapter-node build-adapter-dotnet build-adapter-php build-adapter-python \
        build-adapter-vscode package-adapter-vscode install-adapter-vscode \
        test-adapter-node test-adapter-dotnet test-adapter-php test-adapter-python \
        benchmark benchmark-x gen-fixtures \
        clean-wasm clean-wasi clean-adapter-node clean-adapter-dotnet clean-adapter-php clean-adapter-python clean-all \
        build-all test-all example-all \
        example-node example-dotnet example-php example-python

build-wasm:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building WASM (node + web)..."
	@echo ""
	wasm-pack build src/core --target nodejs --out-dir ../../dist/node
	wasm-pack build src/core --target web    --out-dir ../../dist/web

build-wasi:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building WASI binary..."
	@echo ""
	cargo build --manifest-path src/core-wasi/Cargo.toml --release --target wasm32-wasip1
	mkdir -p dist/wasi && cp target/wasm32-wasip1/release/lpdf-wasi.wasm dist/wasi/lpdf.wasm

test-wasm:
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

benchmark:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running benchmarks (xs–xl)..."
	@echo ""
	cargo bench --manifest-path src/core/Cargo.toml --bench pipeline --bench images --bench fonts -- --output-format bencher parse_xml/ layout/ end_to_end/ data/

benchmark-x:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running extended benchmarks (xxl + max)..."
	@echo ">>> Warning: this may take 10+ minutes."
	@echo ""
	cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --output-format bencher parse_xml_x/ layout_x/ end_to_end_x/

gen-fixtures:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Generating benchmark fixtures..."
	@echo ""
	cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all --data --out test/fixtures

build-adapter-node:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Node adapter..."
	@echo ""
	cd src/adapters/node && npm install && npm run build

test-adapter-node: build-adapter-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Node adapter..."
	@echo ""
	cd src/adapters/node && npm test

build-adapter-dotnet: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building .NET adapter..."
	@echo ""
	mkdir -p src/adapters/dotnet/wasm && cp dist/wasi/lpdf.wasm src/adapters/dotnet/wasm/lpdf.wasm
	cd src/adapters/dotnet && dotnet build Lpdf.csproj -c Release

test-adapter-dotnet: build-adapter-dotnet
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing .NET adapter..."
	@echo ""
	cd src/adapters/dotnet && dotnet test

build-adapter-php: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building PHP adapter..."
	@echo ""
	mkdir -p src/adapters/php/resources && cp dist/wasi/lpdf.wasm src/adapters/php/resources/lpdf-wasi.wasm
	docker build -t lpdf-php src/adapters/php

test-adapter-php: build-adapter-php
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

build-adapter-python: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Python adapter..."
	@echo ""
	mkdir -p src/adapters/python/resources && cp dist/wasi/lpdf.wasm src/adapters/python/resources/lpdf-wasi.wasm
	docker build -t lpdf-python src/adapters/python

test-adapter-python: build-adapter-python
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Python adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/adapters/python/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/adapters/python/tests://app/tests" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		-v "$(CURDIR)/src/adapters/python/resources://app/resources" \
		-w //app lpdf-python python -m pytest tests -v

build-all: build-wasm build-wasi build-adapter-node build-adapter-dotnet build-adapter-php build-adapter-python

test-all: test-wasm test-wasi test-adapter-node test-adapter-dotnet test-adapter-php test-adapter-python

example-all: example-node example-dotnet example-php example-python

clean-wasm:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning WASM artifacts..."
	@echo ""
	cargo clean --manifest-path src/core/Cargo.toml
	rm -rf dist/node dist/web

clean-wasi:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning WASI artifacts..."
	@echo ""
	cargo clean --manifest-path src/core-wasi/Cargo.toml
	rm -rf dist/wasi

clean-adapter-node:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning Node adapter..."
	@echo ""
	rm -rf src/adapters/node/dist src/adapters/node/node_modules

clean-adapter-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning .NET adapter..."
	@echo ""
	cd src/adapters/dotnet && dotnet clean

clean-adapter-php:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning PHP adapter..."
	@echo ""
	docker rmi lpdf-php || true

clean-adapter-python:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning Python adapter..."
	@echo ""
	docker rmi lpdf-python || true

clean-all: clean-wasm clean-wasi clean-adapter-node clean-adapter-dotnet clean-adapter-php clean-adapter-python

example-node:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running Node example..."
	@echo ""
	cd src/adapters/node && npx ts-node example/example.ts && npx ts-node example/encrypt-permissions-only.ts && npx ts-node example/encrypt-open-password.ts && npx ts-node example/example-data.ts

example-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running .NET example..."
	@echo ""
	dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj

example-php:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running PHP example..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/adapters/php/example/example.php
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/adapters/php/example/encrypt-permissions-only.php
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/adapters/php/example/encrypt-open-password.php
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/adapters/php/example/example-data.php
	docker run --rm \
		-v "$(CURDIR)/src/adapters/php/example://app/src/adapters/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/adapters/php/example/example_canvas.php

example-python:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running Python example..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/adapters/python/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/adapters/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/src/adapters/python/resources://app/resources" \
		-w //app lpdf-python python example/example.py
	docker run --rm \
		-v "$(CURDIR)/src/adapters/python/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/adapters/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/docs://app/docs" \
		-v "$(CURDIR)/src/adapters/python/resources://app/resources" \
		-w //app lpdf-python python example/encrypt-permissions-only.py
	docker run --rm \
		-v "$(CURDIR)/src/adapters/python/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/adapters/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/docs://app/docs" \
		-v "$(CURDIR)/src/adapters/python/resources://app/resources" \
		-w //app lpdf-python python example/encrypt-open-password.py
	docker run --rm \
		-v "$(CURDIR)/src/adapters/python/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/adapters/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/src/adapters/python/resources://app/resources" \
		-w //app lpdf-python python example/example-data.py

build-adapter-vscode: build-wasm
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building VS Code adapter..."
	@echo ""
	mkdir -p src/adapters/vscode/wasm && mkdir -p src/adapters/vscode/schema
	cp dist/node/lpdf.js      src/adapters/vscode/wasm/ && cp dist/node/lpdf_bg.wasm src/adapters/vscode/wasm/ && cp dist/node/lpdf.d.ts src/adapters/vscode/wasm/ && cp schema/lpdf.xsd src/adapters/vscode/schema/
	cd src/adapters/vscode && npm install && npm run build

package-adapter-vscode: build-adapter-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Packaging VS Code extension..."
	@echo ""
	cd src/adapters/vscode && npx vsce package --out lpdf.vsix --baseContentUrl https://github.com/codesensedev/lpdf/raw/HEAD/src/adapters/vscode

install-adapter-vscode: package-adapter-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Installing VS Code extension..."
	@echo ""
	code.cmd --install-extension src/adapters/vscode/lpdf.vsix --force
