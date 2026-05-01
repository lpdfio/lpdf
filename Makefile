ifeq ($(OS),Windows_NT)
    SHELL := C:/PROGRA~1/Git/bin/sh.exe
    .SHELLFLAGS := -c
endif

# Auto-load the local dev public key if LPDF_PUBLIC_KEY is not already set.
# Requires src/internal/license/keys/public.hex — run `npm start` in src/internal/license/ once to generate it.
LPDF_PUBLIC_KEY ?= $(shell cat src/internal/license/keys/public.hex 2>/dev/null | tr -d '[:space:]')
export LPDF_PUBLIC_KEY

.PHONY: build-wasm build-wasi test-wasm test-wasi \
        build-sdk-node build-sdk-dotnet build-sdk-php build-sdk-python \
        build-vscode package-vscode install-vscode \
        test-sdk-node test-sdk-dotnet test-sdk-php test-sdk-python \
        benchmark benchmark-x gen-fixtures codegen \
        clean-wasm clean-wasi clean-adapter-node clean-adapter-dotnet clean-adapter-php clean-adapter-python clean-all \
        build-all test-all example-all \
        example-node example-dotnet example-php example-python \
        clone-adapters sync-license check-license \
        build-pages

# ── Pages demo bundle ─────────────────────────────────────────────────────────
# Copies the browser adapter (browser.js) and the WASM binary into the pages
# asset tree so the home-page demo component can be served without traversing
# outside the pages root.  Depends on build-sdk-node so the source files
# are guaranteed to exist before the copy.
# Also runs `npm run build` inside src/pages/ which bundles CodeMirror and
# all other authored JS assets via src/pages/src/build.mjs.
build-pages: build-sdk-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Bundling pages assets..."
	@echo ""
	mkdir -p src/pages/www/assets/js/lpdf
	cp src/sdk/node/dist/browser.js          src/pages/www/assets/js/lpdf/browser.js
	cp src/sdk/node/dist/wasm/lpdf-web.js    src/pages/www/assets/js/lpdf/lpdf-web.js
	cp dist/web/lpdf_bg.wasm                 src/pages/www/assets/js/lpdf/lpdf_bg.wasm
	cd src/pages && npm install && npm run build
	@echo ">>> src/pages/www/assets/js/ updated."

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

test-wasi: test-wasm
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
	cargo bench --manifest-path src/core/Cargo.toml --bench pipeline --bench images --bench fonts -- "parse_xml/|layout/|end_to_end/|data/"

benchmark-x:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running extended benchmarks (xxl + max)..."
	@echo ">>> Warning: this may take 10+ minutes."
	@echo ""
	cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- "parse_xml_x/|layout_x/|end_to_end_x/"

gen-fixtures:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Generating benchmark fixtures..."
	@echo ""
	cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all --data --out test/fixtures

codegen:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running codegen..."
	@echo ""
	@if [ -z "$(INPUT)" ]; then echo "ERROR: INPUT is required. Usage: make codegen INPUT=invoice.xml TARGET=js OUTPUT=out/invoice.ts"; exit 1; fi
	@if [ -z "$(TARGET)" ]; then echo "ERROR: TARGET is required. Usage: make codegen INPUT=invoice.xml TARGET=js OUTPUT=out/invoice.ts"; exit 1; fi
	cargo run --manifest-path src/core/Cargo.toml --bin codegen -- \
		--input $(INPUT) \
		--target $(TARGET) \
		$(if $(OUTPUT),--output $(OUTPUT))

build-sdk-node: build-wasm
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Node adapter..."
	@echo ""
	mkdir -p src/sdk/node/wasm && cp dist/node/lpdf.js dist/node/lpdf.d.ts dist/node/lpdf_bg.wasm src/sdk/node/wasm/ && cp dist/web/lpdf.js src/sdk/node/wasm/lpdf-web.js
	cd src/sdk/node && npm install && npm run build

test-sdk-node: build-sdk-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Node adapter..."
	@echo ""
	cd src/sdk/node && npm test

build-sdk-dotnet: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building .NET adapter..."
	@echo ""
	mkdir -p src/sdk/dotnet/wasm && cp dist/wasi/lpdf.wasm src/sdk/dotnet/wasm/lpdf.wasm
	cd src/sdk/dotnet && dotnet build Lpdf.csproj -c Release

test-sdk-dotnet: build-sdk-dotnet
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing .NET adapter..."
	@echo ""
	cd src/sdk/dotnet && dotnet test

build-sdk-php: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building PHP adapter..."
	@echo ""
	mkdir -p src/sdk/php/resources && cp dist/wasi/lpdf.wasm src/sdk/php/resources/lpdf-wasi.wasm
	docker build -t lpdf-php src/sdk/php

test-sdk-php: build-sdk-php
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing PHP adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/src://app/src" \
		-v "$(CURDIR)/src/sdk/php/test://app/test" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		-v "$(CURDIR)/src/sdk/php/resources://app/resources" \
		-w //app lpdf-php php vendor/bin/phpunit test

build-sdk-python: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Python adapter..."
	@echo ""
	mkdir -p src/sdk/python/resources && cp dist/wasi/lpdf.wasm src/sdk/python/resources/lpdf-wasi.wasm
	docker build -t lpdf-python src/sdk/python

test-sdk-python: build-sdk-python
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Python adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/sdk/python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/sdk/python/tests://app/tests" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		-v "$(CURDIR)/src/sdk/python/resources://app/resources" \
		-w //app lpdf-python python -m pytest tests -v

build-all: build-wasm build-wasi build-sdk-node build-sdk-dotnet build-sdk-php build-sdk-python

test-all: test-wasm test-wasi test-sdk-node test-sdk-dotnet test-sdk-php test-sdk-python

example-all: example-node example-dotnet example-php example-python

clone-adapters:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cloning adapter repos into src/sdk/..."
	@echo ""
	git clone https://github.com/lpdfio/lpdf-js     src/sdk/node
	git clone https://github.com/lpdfio/lpdf-dotnet src/sdk/dotnet
	git clone https://github.com/lpdfio/lpdf-python src/sdk/python
	git clone https://github.com/lpdfio/lpdf-php    src/sdk/php
	git clone https://github.com/lpdfio/lpdf-vscode src/sdk/vscode

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
	rm -rf src/sdk/node/dist src/sdk/node/node_modules

clean-adapter-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning .NET adapter..."
	@echo ""
	cd src/sdk/dotnet && dotnet clean

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
	cd src/sdk/node && npx ts-node example/example.ts&& npx ts-node example/example2.ts && npx ts-node example/encrypt-permissions-only.ts && npx ts-node example/encrypt-open-password.ts && npx ts-node example/example-data.ts

example-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running .NET example..."
	@echo ""
	dotnet run --project src/sdk/dotnet/example/LpdfExample.csproj

example-php:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running PHP example..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/example://app/src/sdk/php/example" \
		-v "$(CURDIR)/src/sdk/php/lpdf-light.png://app/src/sdk/php/lpdf-light.png" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/sdk/php/example/example.php
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/example://app/src/sdk/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-w //app lpdf-php php src/sdk/php/example/encrypt-permissions-only.php
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/example://app/src/sdk/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-w //app lpdf-php php src/sdk/php/example/encrypt-open-password.php
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/example://app/src/sdk/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/sdk/php/example/example-data.php
	docker run --rm \
		-v "$(CURDIR)/src/sdk/php/example://app/src/sdk/php/example" \
		-v "$(CURDIR)/example://app/example" \
		-v "$(CURDIR)/docs://app/docs" \
		-w //app lpdf-php php src/sdk/php/example/example_canvas.php

example-python:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running Python example..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/src/sdk/python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/sdk/python/example://app/example" \
		-v "$(CURDIR)/src/sdk/python/lpdf-light.png://app/lpdf-light.png" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/src/sdk/python/resources://app/resources" \
		-w //app lpdf-python python example/example.py
	docker run --rm \
		-v "$(CURDIR)/src/sdk/python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/sdk/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/src/sdk/python/resources://app/resources" \
		-w //app lpdf-python python example/encrypt-permissions-only.py
	docker run --rm \
		-v "$(CURDIR)/src/sdk/python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/sdk/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/src/sdk/python/resources://app/resources" \
		-w //app lpdf-python python example/encrypt-open-password.py
	docker run --rm \
		-v "$(CURDIR)/src/sdk/python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/src/sdk/python/example://app/example" \
		-v "$(CURDIR)/example://app/example-data" \
		-v "$(CURDIR)/src/sdk/python/resources://app/resources" \
		-w //app lpdf-python python example/example-data.py

build-vscode: build-wasm
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building VS Code adapter..."
	@echo ""
	mkdir -p src/sdk/vscode/wasm && mkdir -p src/sdk/vscode/schema
	cp dist/node/lpdf.js      src/sdk/vscode/wasm/ && cp dist/node/lpdf_bg.wasm src/sdk/vscode/wasm/ && cp dist/node/lpdf.d.ts src/sdk/vscode/wasm/ && cp schema/lpdf.xsd src/sdk/vscode/schema/
	cd src/sdk/vscode && npm install && npm run build

package-vscode: build-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Packaging VS Code extension..."
	@echo ""
	cd src/sdk/vscode && npx vsce package --out lpdf.vsix --baseContentUrl https://github.com/lpdfio/lpdf-vscode/raw/HEAD

install-vscode: package-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Installing VS Code extension..."
	@echo ""
	code.cmd --install-extension src/sdk/vscode/lpdf.vsix --force

sync-license:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Syncing LICENSE from root to all copies..."
	@echo ""
	@$(SHELL) -c "cp LICENSE src/core/LICENSE"
	@$(SHELL) -c "cp LICENSE src/public/pages/content/LICENSE.md"
	@$(SHELL) -c "test -d src/sdk/node    && cp LICENSE src/sdk/node/LICENSE    || true"
	@$(SHELL) -c "test -d src/sdk/dotnet  && cp LICENSE src/sdk/dotnet/LICENSE  || true"
	@$(SHELL) -c "test -d src/sdk/php     && cp LICENSE src/sdk/php/LICENSE     || true"
	@$(SHELL) -c "test -d src/sdk/python  && cp LICENSE src/sdk/python/LICENSE  || true"
	@$(SHELL) -c "test -d src/sdk/vscode  && cp LICENSE src/sdk/vscode/LICENSE  || true"
	@echo "Done. Commit changes in each adapter repo separately."

check-license:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Checking LICENSE copies are in sync..."
	@echo ""
	@$(SHELL) -c "diff LICENSE src/core/LICENSE          || (echo 'ERROR: src/core/LICENSE differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "diff LICENSE src/public/pages/content/LICENSE.md  || (echo 'ERROR: src/public/pages/content/LICENSE.md differs from root LICENSE' && exit 1)"
	@echo "OK"
