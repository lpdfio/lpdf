ifeq ($(OS),Windows_NT)
    SHELL := C:/PROGRA~1/Git/bin/sh.exe
    .SHELLFLAGS := -c
endif

# Auto-load the local dev public key if LPDF_PUBLIC_KEY is not already set.
# Requires repos/lpdf-license/keys/public.hex — run `npm start` in repos/lpdf-license/ once to generate it.
LPDF_PUBLIC_KEY ?= $(shell cat repos/lpdf-license/keys/public.hex 2>/dev/null | tr -d '[:space:]')
export LPDF_PUBLIC_KEY

# The lpdf.io site, checked out under repos/, and the portal UI outside this repo that builds its bundles.
PAGES_DIR     ?= repos/lpdf-pages
PORTAL_UI_DIR ?= ../codesense/portal/ui

.PHONY: build-wasm build-wasi build-cli test-wasm test-wasi test-cli \
        build-sdk-node build-sdk-dotnet build-sdk-php build-sdk-python \
        build-vscode package-vscode install-vscode \
        test-sdk-node test-sdk-dotnet test-sdk-php test-sdk-python \
        benchmark benchmark-x gen-fixtures codegen gen-sdk-api check-sdk-api check-encryption \
        clean-wasm clean-wasi clean-adapter-node clean-adapter-dotnet clean-adapter-php clean-adapter-python clean-all \
        build-all test-all example-all \
        example-node example-dotnet example-php example-python \
        clone-adapters sync-license check-license \
        build-portal-ui build-pages-demo build-pages build-pages-docs check-pages-docs stamp-pages-docs dev-pages meta-pages \
        sync-examples check-examples-sync build-demo-viewer \
        rc-next rc-check rc-vsix

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

build-cli:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building CLI (native)..."
	@echo ""
	cargo build --manifest-path src/cli/Cargo.toml --release
ifeq ($(OS),Windows_NT)
	mkdir -p bin && cp target/release/lpdf.exe bin/lpdf.exe
else
	mkdir -p bin && cp target/release/lpdf bin/lpdf
endif

test-wasm:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running core tests..."
	@echo ""
	cargo test --manifest-path src/core/Cargo.toml

# check-encryption has the core write encrypted PDFs and opens them with qpdf, which shares no code with the
# engine: each password opens its file, a wrong one is refused, and decrypting gives the unencrypted render
# (`scripts/check-encryption.mjs`). Needs qpdf on the PATH: `apt install qpdf` or `brew install qpdf`.
check-encryption:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Checking encrypted PDFs with qpdf..."
	@echo ""
	rm -rf target/encryption-samples && mkdir -p target/encryption-samples
	LPDF_ENCRYPT_OUT="$(CURDIR)/target/encryption-samples" cargo test --manifest-path src/core/Cargo.toml --lib writes_samples_for_the_external_check
	node scripts/check-encryption.mjs target/encryption-samples

test-wasi: test-wasm
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running WASI tests..."
	@echo ""
	cargo test --manifest-path src/core-wasi/Cargo.toml

test-cli:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running CLI tests..."
	@echo ""
	cargo test --manifest-path src/cli/Cargo.toml

benchmark:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running benchmarks (xs–xl)..."
	@echo ""
	cargo bench --manifest-path src/core/Cargo.toml --bench pipeline --bench images --bench fonts -- "parse_xml/|layout/|end_to_end/|data/|layers/|images/|qr/|linear/|subsetting/|image/"

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
	mkdir -p repos/lpdf-js/wasm && cp dist/node/lpdf.js dist/node/lpdf.d.ts dist/node/lpdf_bg.wasm repos/lpdf-js/wasm/ && cp dist/web/lpdf.js repos/lpdf-js/wasm/lpdf-web.js
	cd repos/lpdf-js && npm install && npm run build

test-sdk-node: build-sdk-node
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Node adapter..."
	@echo ""
	cd repos/lpdf-js && npm test

build-sdk-dotnet: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building .NET adapter..."
	@echo ""
	mkdir -p repos/lpdf-dotnet/wasm && cp dist/wasi/lpdf.wasm repos/lpdf-dotnet/wasm/lpdf.wasm
	cd repos/lpdf-dotnet && dotnet build Lpdf.csproj -c Release

test-sdk-dotnet: build-sdk-dotnet
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing .NET adapter..."
	@echo ""
	cd repos/lpdf-dotnet && dotnet test

# The PHP repo commits resources/lpdf-wasi.wasm, the release build that Packagist serves.
# A local build trusts only the dev key, so it must never land there: local runs mount it
# over the image's copy instead.
PHP_WASM_MOUNT = -v "$(CURDIR)/dist/wasi/lpdf.wasm://app/resources/lpdf-wasi.wasm"

build-sdk-php: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building PHP adapter..."
	@echo ""
	docker build -t lpdf-php repos/lpdf-php

test-sdk-php: build-sdk-php
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing PHP adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-php/src://app/src" \
		-v "$(CURDIR)/repos/lpdf-php/test://app/test" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		$(PHP_WASM_MOUNT) \
		-w //app lpdf-php php vendor/bin/phpunit test

build-sdk-python: build-wasi
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building Python adapter..."
	@echo ""
	mkdir -p repos/lpdf-python/resources && cp dist/wasi/lpdf.wasm repos/lpdf-python/resources/lpdf-wasi.wasm
	docker build -t lpdf-python repos/lpdf-python

test-sdk-python: build-sdk-python
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Testing Python adapter..."
	@echo ""
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/repos/lpdf-python/tests://app/tests" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/test/snapshots://app/test/snapshots" \
		-v "$(CURDIR)/repos/lpdf-python/resources://app/resources" \
		-w //app lpdf-python python -m pytest tests -v

build-all: clean-all build-wasm build-wasi build-sdk-node build-sdk-dotnet build-sdk-php build-sdk-python build-vscode build-pages

# The attribute classes and constants of the four SDKs follow from schema/lpdf.xsd. gen-sdk-api writes them;
# check-sdk-api fails if a generated file differs from what the schema gives, or if any SDK class, written by
# hand or generated, has other attributes than the schema's element.
gen-sdk-api:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Generating the SDK attribute classes and constants from the schema..."
	@echo ""
	node scripts/gen-sdk-api.mjs

check-sdk-api:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Checking the SDKs against the schema..."
	@echo ""
	node scripts/gen-sdk-api.mjs --check
	node scripts/check-sdk-parity.mjs

test-all: check-sdk-api test-wasm test-wasi test-cli test-sdk-node test-sdk-dotnet test-sdk-php test-sdk-python

example-all: example-node example-dotnet example-php example-python

clone-adapters:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cloning adapter repos into repos/..."
	@echo ""
	git clone https://github.com/lpdfio/lpdf-js     repos/lpdf-js
	git clone https://github.com/lpdfio/lpdf-dotnet repos/lpdf-dotnet
	git clone https://github.com/lpdfio/lpdf-python repos/lpdf-python
	git clone https://github.com/lpdfio/lpdf-php    repos/lpdf-php
	git clone https://github.com/lpdfio/lpdf-vscode repos/lpdf-vscode

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
	rm -rf repos/lpdf-js/dist repos/lpdf-js/node_modules

clean-adapter-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Cleaning .NET adapter..."
	@echo ""
	cd repos/lpdf-dotnet && dotnet clean

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
	cd repos/lpdf-js && npx ts-node example/example.ts&& npx ts-node example/example2.ts && npx ts-node example/encrypt-permissions-only.ts && npx ts-node example/encrypt-open-password.ts && npx ts-node example/example-data.ts

example-dotnet:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running .NET example..."
	@echo ""
	dotnet run --project repos/lpdf-dotnet/example/LpdfExample.csproj

example-php:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running PHP example..."
	@echo ""
	mkdir -p tmp/sdk-examples
	docker run --rm \
		$(PHP_WASM_MOUNT) \
		-v "$(CURDIR)/repos/lpdf-php/example://app/repos/lpdf-php/example" \
		-v "$(CURDIR)/examples://app/examples" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-php php repos/lpdf-php/example/example.php
	docker run --rm \
		$(PHP_WASM_MOUNT) \
		-v "$(CURDIR)/repos/lpdf-php/example://app/repos/lpdf-php/example" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-php php repos/lpdf-php/example/encrypt-permissions-only.php
	docker run --rm \
		$(PHP_WASM_MOUNT) \
		-v "$(CURDIR)/repos/lpdf-php/example://app/repos/lpdf-php/example" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-php php repos/lpdf-php/example/encrypt-open-password.php
	docker run --rm \
		$(PHP_WASM_MOUNT) \
		-v "$(CURDIR)/repos/lpdf-php/example://app/repos/lpdf-php/example" \
		-v "$(CURDIR)/examples://app/examples" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-php php repos/lpdf-php/example/example-data.php
	docker run --rm \
		$(PHP_WASM_MOUNT) \
		-v "$(CURDIR)/repos/lpdf-php/example://app/repos/lpdf-php/example" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-php php repos/lpdf-php/example/example_canvas.php

example-python:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Running Python example..."
	@echo ""
	mkdir -p tmp/sdk-examples
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/repos/lpdf-python/example://app/example" \
		-v "$(CURDIR)/repos/lpdf-python/resources://app/resources" \
		-v "$(CURDIR)/examples://app/examples" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-python python example/example.py
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/repos/lpdf-python/example://app/example" \
		-v "$(CURDIR)/repos/lpdf-python/resources://app/resources" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-python python example/encrypt-permissions-only.py
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/repos/lpdf-python/example://app/example" \
		-v "$(CURDIR)/repos/lpdf-python/resources://app/resources" \
		-v "$(CURDIR)/test/fixtures://app/test/fixtures" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-python python example/encrypt-open-password.py
	docker run --rm \
		-v "$(CURDIR)/repos/lpdf-python/src/lpdf://app/lpdf" \
		-v "$(CURDIR)/repos/lpdf-python/example://app/example" \
		-v "$(CURDIR)/repos/lpdf-python/resources://app/resources" \
		-v "$(CURDIR)/examples://app/examples" \
		-v "$(CURDIR)/tmp/sdk-examples://app/tmp/sdk-examples" \
		-w //app lpdf-python python example/example-data.py

build-vscode: build-wasm
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Building VS Code adapter..."
	@echo ""
	mkdir -p repos/lpdf-vscode/wasm && mkdir -p repos/lpdf-vscode/schema
	cp dist/node/lpdf.js      repos/lpdf-vscode/wasm/ && cp dist/node/lpdf_bg.wasm repos/lpdf-vscode/wasm/ && cp dist/node/lpdf.d.ts repos/lpdf-vscode/wasm/ && cp schema/lpdf.xsd repos/lpdf-vscode/schema/
	cd repos/lpdf-vscode && npm install && npm run build

package-vscode: build-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Packaging VS Code extension..."
	@echo ""
	cd repos/lpdf-vscode && npx vsce package --out lpdf.vsix --baseContentUrl https://github.com/lpdfio/lpdf-vscode/raw/HEAD

# `code`, not code.cmd: the Mac has only `code`, and on Windows Git's sh runs the `code` script that sits next to
# code.cmd. It has to be on PATH (on the Mac: "Install 'code' command in PATH" in VS Code's command palette).
install-vscode: package-vscode
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Installing VS Code extension..."
	@echo ""
	code --install-extension repos/lpdf-vscode/lpdf.vsix --force

sync-license:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Syncing LICENSE from root to all copies..."
	@echo ""
	@$(SHELL) -c "cp LICENSE src/core/LICENSE"
	@$(SHELL) -c "test -d '$(PAGES_DIR)'  && cp LICENSE '$(PAGES_DIR)/www/content/LICENSE.md' || true"
	@$(SHELL) -c "test -d repos/lpdf-js      && cp LICENSE repos/lpdf-js/LICENSE      || true"
	@$(SHELL) -c "test -d repos/lpdf-dotnet  && cp LICENSE repos/lpdf-dotnet/LICENSE  || true"
	@$(SHELL) -c "test -d repos/lpdf-php     && cp LICENSE repos/lpdf-php/LICENSE     || true"
	@$(SHELL) -c "test -d repos/lpdf-python  && cp LICENSE repos/lpdf-python/LICENSE  || true"
# repos/lpdf-vscode is not synced: the extension has its own two-part LICENSE (MIT code + engine terms).
	@echo "Done. Commit changes in the pages repo and each adapter repo separately."

check-license:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Checking LICENSE copies are in sync..."
	@echo ""
	@$(SHELL) -c "diff LICENSE src/core/LICENSE          || (echo 'ERROR: src/core/LICENSE differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "test ! -d '$(PAGES_DIR)'  || diff LICENSE '$(PAGES_DIR)/www/content/LICENSE.md' || (echo 'ERROR: $(PAGES_DIR)/www/content/LICENSE.md differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "test ! -d repos/lpdf-js      || diff LICENSE repos/lpdf-js/LICENSE      || (echo 'ERROR: repos/lpdf-js/LICENSE differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "test ! -d repos/lpdf-dotnet  || diff LICENSE repos/lpdf-dotnet/LICENSE  || (echo 'ERROR: repos/lpdf-dotnet/LICENSE differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "test ! -d repos/lpdf-php     || diff LICENSE repos/lpdf-php/LICENSE     || (echo 'ERROR: repos/lpdf-php/LICENSE differs from root LICENSE' && exit 1)"
	@$(SHELL) -c "test ! -d repos/lpdf-python  || diff LICENSE repos/lpdf-python/LICENSE  || (echo 'ERROR: repos/lpdf-python/LICENSE differs from root LICENSE' && exit 1)"
	@echo "OK"

# ── Release candidate ─────────────────────────────────────────────────────────
# Interactive: works out the next vX.Y.0-rc.N from GitHub's tags, shows the
# commit and what changed, and pushes the tag only after you confirm. The push
# starts release.yml's RC job; nothing reaches a package registry. The
# "Cut release candidate" button on GitHub's Actions tab does the same.
# Lpdf-devcycle.md §4.2.
#   make rc-next                  next minor after the latest release
#   make rc-next VERSION=v1.0.0   a specific version, e.g. a major bump
rc-next:
	@$(SHELL) scripts/rc-next.sh "$(VERSION)"

# Read-only status of a release candidate: core's run and the five rc.yml runs,
# what failed and why, and, when all passed, the pre-filled release link.
#   make rc-check                   the newest RC on GitHub
#   make rc-check RC=v0.19.0-rc.2   a specific one
rc-check:
	@node scripts/rc-check.mjs "$(RC)"

# Downloads the .vsix that lpdf-vscode's rc.yml built for a release candidate
# into repos/lpdf-vscode/.rc/<tag>/, and prints how to install and check it. Needs gh.
#   make rc-vsix                    the newest RC lpdf-vscode built
#   make rc-vsix RC=v0.25.0-rc.2    a specific one
rc-vsix:
	@node scripts/rc-vsix.mjs "$(RC)"

# ── Portal UI bundle ───────────────────────────────────────────────────────────
# build:pages writes lpdf-pages*.js (one bundle for each kind of page) and lpdf-docs.js
# straight into the pages asset tree with __PORTAL_URL__ / __PAGES_URL__ sentinels,
# ignoring .env.local.  The pages deploy workflow owns the sed replacement for each
# environment.
build-portal-ui:
	cd "$(PORTAL_UI_DIR)" && npm run build:pages

# ── Pages demo bundle ─────────────────────────────────────────────────────────
# Builds the standalone demo sub-project in $(PAGES_DIR)/ui/.
build-pages-demo:
	cd "$(PAGES_DIR)/ui" && CI=true npm run build

# ── Pages SEO meta sync ───────────────────────────────────────────────────────
meta-pages:
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Syncing SEO meta from sitemap.yaml..."
	@echo ""
	node "$(PAGES_DIR)/update-meta.mjs"

# ── Pages docs stamp ──────────────────────────────────────────────────────────
# Records which core the docs describe (the last release, how far this checkout is past it, the commit)
# in docs-site/engine.json. The docs header shows it beside the revision of the pages build. Commit the
# file with the assets it describes. build-pages and dev-pages run this, since they are what copy the
# engine into the pages tree.
stamp-pages-docs:
	@test -d "$(PAGES_DIR)/docs-site" || (echo "ERROR: docs-site not found at $(PAGES_DIR)/docs-site" && exit 1)
	node scripts/stamp-docs.mjs "$(PAGES_DIR)/docs-site/engine.json"

# ── Examples ─────────────────────────────────────────────────────────────────
# examples/ holds the seven examples, from the admission letter to the book, and is the one source of them. The VS Code
# extension offers them as templates (repos/lpdf-vscode/templates), and the demo on lpdf.io and the docs show them (both in
# $(PAGES_DIR)); sync-examples copies them to all three, and build-pages runs it. The docs build renders each one to
# a PDF itself, with the engine of the site, for the viewer on its page. check-examples-sync fails if a copy is not what
# sync-examples would write.
sync-examples:
	node scripts/sync-examples.mjs

check-examples-sync:
	node scripts/sync-examples.mjs --check

# The demo's PDF viewer is the extension's, so a change to the viewer or its page reaches the demo through this.
# It builds the extension's TypeScript, which has the function that makes the page, but not its engine.
build-demo-viewer:
	cd repos/lpdf-vscode && npm run build
	node scripts/build-demo-viewer.mjs

# ── Pages docs site ───────────────────────────────────────────────────────────
# The Starlight docs in $(PAGES_DIR)/docs-site. The pages deploy workflow builds them itself; this is for
# looking at them locally, where lpdf.local/docs serves docs-site/dist. The code tabs and the example
# check read the demo WASM that build-pages copies into the pages tree, so after an engine change run
# build-pages first.
build-pages-docs:
	@test -d "$(PAGES_DIR)/docs-site" || (echo "ERROR: docs-site not found at $(PAGES_DIR)/docs-site" && exit 1)
	cd "$(PAGES_DIR)/docs-site" && (test -d node_modules || npm ci) && npm run build

# Renders every XML example in the docs with the engine copy in the pages tree, and fails if the engine
# rejects one. It then runs the code the generator writes for each one with the SDK that build-sdk-node
# built in repos/lpdf-js, so the docs of an element that is not released yet pass. Without that build it
# uses the published SDK in the docs site's node_modules, which needs npm ci.
check-pages-docs:
	@test -d "$(PAGES_DIR)/docs-site" || (echo "ERROR: docs-site not found at $(PAGES_DIR)/docs-site" && exit 1)
	cd "$(PAGES_DIR)/docs-site" && $(if $(wildcard repos/lpdf-js/dist/index.js),LPDF_SDK_DIR="$(abspath repos/lpdf-js)" )npm run check

# ── Pages asset sync ──────────────────────────────────────────────────────────
# Copies the browser WASM build and the demo bundle+assets into the pages asset tree.
# The portal bundles are already in place: build-portal-ui writes them there.
# It ends by stamping the docs with this core and rendering their examples with the refreshed engine,
# so an engine change that breaks one fails here, and the docs get fixed in the same change.
build-pages: build-portal-ui sync-examples build-demo-viewer build-pages-demo
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Copying pages assets..."
	@echo ""
	@test -d "$(PAGES_DIR)/www" || (echo "ERROR: pages checkout not found at $(PAGES_DIR)" && exit 1)
	cp dist/web/lpdf.js "$(PAGES_DIR)/ui/demo/lpdf-web.js" && \
	cp dist/web/lpdf_bg.wasm "$(PAGES_DIR)/ui/demo/lpdf_bg.wasm" && \
	echo ">>> $(PAGES_DIR)/ui/demo WASM updated." && \
	rm -rf tmp/lpdf-demo && mkdir -p tmp/lpdf-demo && \
	cp -r "$(PAGES_DIR)/ui/demo/." tmp/lpdf-demo/ && \
	cp "$(PAGES_DIR)/ui/dist/lpdf-demo.js" tmp/lpdf-demo/lpdf-demo.js && \
	rm -rf "$(PAGES_DIR)/www/assets/js/lpdf-demo" && \
	cp -r tmp/lpdf-demo "$(PAGES_DIR)/www/assets/js/lpdf-demo" && \
	echo ">>> $(PAGES_DIR)/www/assets/js/lpdf-demo updated." && \
	mkdir -p "$(PAGES_DIR)/www/schema/1" && \
	cp schema/lpdf.xsd "$(PAGES_DIR)/www/schema/1/lpdf.xsd" && \
	cp schema/lpdf.xsd "$(PAGES_DIR)/www/schema/lpdf.xsd" && \
	echo ">>> $(PAGES_DIR)/www/schema updated (versioned at /1/, latest at the root)."
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Stamping the docs with this core, and checking their examples against it..."
	@echo ""
	@$(MAKE) --no-print-directory stamp-pages-docs
	@$(MAKE) --no-print-directory check-pages-docs

# Local-dev variant: a plain portal build bakes .env.local values in, so its
# bundles are copied over the committed ones.  Use this to test pages locally.
# Do NOT commit the output — run make build-pages before committing to restore
# sentinel values for CI deployment.
dev-pages: sync-examples build-demo-viewer
	@test -d "$(PAGES_DIR)/www" || (echo "ERROR: pages checkout not found at $(PAGES_DIR)" && exit 1)
	cd "$(PORTAL_UI_DIR)" && npm run build
	cd "$(PAGES_DIR)/ui" && npm run build
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Copying pages assets (local dev)..."
	@echo ""
	cp "$(PORTAL_UI_DIR)"/dist/lpdf-pages*.js "$(PAGES_DIR)/www/assets/js/" && \
	echo ">>> $(PAGES_DIR)/www/assets/js/lpdf-pages*.js updated." && \
	cp "$(PORTAL_UI_DIR)/dist/lpdf-docs.js" "$(PAGES_DIR)/www/assets/js/lpdf-docs.js" && \
	echo ">>> $(PAGES_DIR)/www/assets/js/lpdf-docs.js updated." && \
	cp dist/web/lpdf.js "$(PAGES_DIR)/ui/demo/lpdf-web.js" && \
	cp dist/web/lpdf_bg.wasm "$(PAGES_DIR)/ui/demo/lpdf_bg.wasm" && \
	echo ">>> $(PAGES_DIR)/ui/demo WASM updated." && \
	rm -rf tmp/lpdf-demo && mkdir -p tmp/lpdf-demo && \
	cp -r "$(PAGES_DIR)/ui/demo/." tmp/lpdf-demo/ && \
	cp "$(PAGES_DIR)/ui/dist/lpdf-demo.js" tmp/lpdf-demo/lpdf-demo.js && \
	rm -rf "$(PAGES_DIR)/www/assets/js/lpdf-demo" && \
	cp -r tmp/lpdf-demo "$(PAGES_DIR)/www/assets/js/lpdf-demo" && \
	echo ">>> $(PAGES_DIR)/www/assets/js/lpdf-demo updated." && \
	mkdir -p "$(PAGES_DIR)/www/schema/1" && \
	cp schema/lpdf.xsd "$(PAGES_DIR)/www/schema/1/lpdf.xsd" && \
	cp schema/lpdf.xsd "$(PAGES_DIR)/www/schema/lpdf.xsd" && \
	echo ">>> $(PAGES_DIR)/www/schema updated (versioned at /1/, latest at the root)."
	@echo ""
	@echo "-------------------------------"
	@echo ">>> Stamping and building the docs site (local dev)..."
	@echo ""
	@$(MAKE) --no-print-directory stamp-pages-docs
	@$(MAKE) --no-print-directory build-pages-docs
