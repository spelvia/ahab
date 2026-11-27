# Common tasks. `make` lists them.

COMPOSE := docker compose --file docker/compose.yaml
# The Postgres server `make test-postgres` creates its test databases on
# (see docker/compose.yaml).
TEST_POSTGRES_URL := postgres://ahab:ahab@127.0.0.1:55443/ahab
# The end-to-end tests' config and data folder, recreated on every run.
E2E_DIR := target/e2e

.DEFAULT_GOAL := help

.PHONY: help
help: ## List the tasks
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN { FS = ":.*## " } { printf "  %-14s %s\n", $$1, $$2 }'

.PHONY: setup
setup: ## Install dependencies and create .env
	pnpm install
	test -f .env || cp .env.example .env

.PHONY: db
db: ## Start Postgres, for running the server on it instead of SQLite
	$(COMPOSE) up --detach --wait postgres

.PHONY: db-test
db-test: ## Start the Postgres that `make test-postgres` uses, to run some tests on it by hand
	$(COMPOSE) up --detach --wait postgres-test

.PHONY: dev
dev: ## Start the server and the Vite dev server
	$(MAKE) --no-print-directory --jobs=2 server ui

.PHONY: server
server: ## Run the server: `ahab up --foreground`
	cargo run --bin ahab -- up --foreground

.PHONY: ui
ui: ## Run the Vite dev server
	pnpm --filter @ahab/ui dev

.PHONY: docs
docs: ## Run the documentation site
	pnpm --filter @ahab/docs dev

.PHONY: preview
preview: ## Build the UI into a release binary and run it, as in production
	pnpm --filter @ahab/ui build
	cargo run --release --bin ahab -- up --foreground

.PHONY: grammars-update
grammars-update: ## Rebuild the bundled grammars and queries from their pinned sources (HELIX=master moves the pin)
	node apps/ui/scripts/grammars-update.mjs $(if $(HELIX),--helix $(HELIX)) $(if $(ONLY),--only $(ONLY))

.PHONY: tools-update
tools-update: ## Pin the newest version of each tool in tools/registry, with checksums (ONLY=id,id for some)
	node scripts/tools-update.mjs $(if $(ONLY),--only $(ONLY))

.PHONY: plugin-fixtures
plugin-fixtures: ## Build the test plugins the server tests install
	pnpm --filter './crates/ahab/tests/fixtures/plugins/*' build

# Full test and lint runs gain little from incremental data but write a lot
# of it; the editor and `make dev` keep it.
test test-postgres lint: export CARGO_INCREMENTAL := 0

.PHONY: test
test: plugin-fixtures ## Run the server tests (on SQLite), the UI tests and the plugin SDK's
	cargo nextest run --workspace --locked
	pnpm --filter @ahab/ui test
	pnpm --filter @ahab/plugin test

.PHONY: test-postgres
test-postgres: plugin-fixtures ## Run the server tests on a throwaway Postgres, then stop it
	@# A fresh server each run: its data is in memory, so recreating it
	@# clears the last run's and stopping it frees the memory. Each test
	@# drops its own database when it ends (tests/support/database.rs).
	$(COMPOSE) up --detach --wait --force-recreate postgres-test
	AHAB_TEST_POSTGRES_URL=$(TEST_POSTGRES_URL) cargo nextest run --workspace --locked; \
		status=$$?; $(COMPOSE) stop postgres-test; exit $$status

.PHONY: e2e
e2e: ## Run the end-to-end tests on a fresh database
	rm -rf $(E2E_DIR)
	pnpm --filter @ahab/ui build
	cargo build --bin ahab --locked
	pnpm --filter @ahab/e2e test

.PHONY: lint
lint: ## Run every linter and check
	cargo fmt --all -- --check
	cargo clippy --all-targets --locked -- -D warnings
	cargo deny check
	cargo deny --manifest-path $(NATIVE_RUST)/Cargo.toml check
	cargo fmt --manifest-path $(NATIVE_RUST)/Cargo.toml -- --check
	cargo machete
	cargo sort --workspace --check --no-format
	cargo sort --check --no-format $(NATIVE_RUST)
	taplo fmt --check
	pnpm exec oxfmt --check
	pnpm exec oxlint --deny-warnings
	pnpm --filter @ahab/ui typecheck
	pnpm --filter @ahab/ui check:layout
	pnpm --filter @ahab/e2e typecheck
	pnpm --filter @ahab/plugin typecheck
	pnpm exec knip
	typos
	gitleaks git --redact --no-banner
	zizmor --offline .github/
	lychee --offline $$(git ls-files '*.md' '*.mdx')

.PHONY: clean-cache
clean-cache: ## Delete build output unused for 14 days or built by another toolchain
	cargo sweep --time 14 .
	cargo sweep --toolchains "$$(rustup show active-toolchain | cut -d' ' -f1)" .
	cargo sweep --time 14 $(NATIVE_RUST)
	cd $(NATIVE_RUST) && cargo sweep --toolchains "$$(rustup show active-toolchain | cut -d' ' -f1)" .

.PHONY: fmt
fmt: ## Format all code
	cargo fmt --all
	cargo fmt --manifest-path $(NATIVE_RUST)/Cargo.toml
	cargo sort --workspace --no-format
	cargo sort --no-format $(NATIVE_RUST)
	taplo fmt
	pnpm exec oxfmt

.PHONY: links
links: ## Check every link in the Markdown, web addresses included
	lychee $$(git ls-files '*.md' '*.mdx')

.PHONY: openapi
openapi: ## Regenerate openapi.json and the UI's API client
	UPDATE_OPENAPI=1 cargo nextest run --locked -E 'test(the_openapi_document_is_current)'
	pnpm --filter @ahab/ui generate:api

# Where `make release` puts the archive for scripts/install.sh.
DIST := target/dist

.PHONY: release
release: ## Build a release archive for this computer: ahab-<os>-<arch>.tar.gz and its SHA-256
	pnpm --filter @ahab/ui build
	cargo build --release --locked --bin ahab
	mkdir -p $(DIST)
	os=$$(uname -s | sed 's/Linux/linux/; s/Darwin/macos/'); \
	arch=$$(uname -m | sed 's/amd64/x86_64/; s/arm64/aarch64/'); \
	name=ahab-$$os-$$arch.tar.gz; \
	tar -czf $(DIST)/$$name -C target/release ahab; \
	cd $(DIST) && sha256sum $$name > $$name.sha256 && echo "$(DIST)/$$name"


.PHONY: build
build: ## Build the Docker image
	docker build --file docker/Dockerfile --tag ahab:local .

# The desktop app: one Tauri project for desktop now and Android
# later. It needs WebKitGTK and the tauri CLI, so these targets run in a
# shell that has them (see README.md).
NATIVE := apps/native
NATIVE_RUST := $(NATIVE)
HOST_TRIPLE = $(shell rustc -vV | sed -n 's/^host: //p')
# Tauri's sidecar: the release `ahab` built from this commit.
SIDECAR = $(NATIVE_RUST)/binaries/ahab-$(HOST_TRIPLE)
# Where `make app-sandbox` keeps its pretend computer.
SANDBOX ?= /tmp/ahab-app-home
SANDBOX_PORT ?= 8187

.PHONY: app-sidecar
app-sidecar: ## Build the release ahab (with the web app) and copy it in as the app's sidecar
	pnpm --filter @ahab/ui build
	cargo build --release --locked --bin ahab
	mkdir -p $(NATIVE_RUST)/binaries
	cp target/release/ahab $(SIDECAR)

.PHONY: app
app: app-sidecar ## Build the desktop app's Linux packages: AppImage, .deb and .rpm
	cd $(NATIVE_RUST) && cargo tauri build
	@find $(NATIVE_RUST)/target/release/bundle \( -name '*.AppImage' -o -name '*.deb' -o -name '*.rpm' \) -exec ls -lh {} \;

.PHONY: app-portable
app-portable: ## Build the desktop app's packages in Ubuntu 22.04 (Docker), to run on most distributions
	DOCKER_BUILDKIT=1 docker build --file docker/app.Dockerfile \
		--output type=local,dest=target/app-dist .
	@ls -lh target/app-dist

.PHONY: app-dev
app-dev: app-sidecar ## Run the desktop app in development (its data in target/app-dev), e.g. against `make dev`
	mkdir -p target/app-dev
	cd $(NATIVE_RUST) && AHAB_APP_DATA_DIR=$(CURDIR)/target/app-dev cargo tauri dev

.PHONY: app-test
app-test: app-sidecar ## Run the desktop app's Rust tests (its screens are the web app's, tested by `make test`)
	cargo test --locked --manifest-path $(NATIVE_RUST)/Cargo.toml

.PHONY: app-lint
app-lint: ## Lint the desktop app's Rust side (needs WebKitGTK; `make lint` checks the rest)
	test -f $(SIDECAR) || $(MAKE) --no-print-directory app-sidecar
	cargo clippy --locked --all-targets --manifest-path $(NATIVE_RUST)/Cargo.toml -- -D warnings

.PHONY: android
android: ## Build the Android app's debug APK for the emulator (needs the Android SDK and NDK)
	cd $(NATIVE) && cargo tauri android build --debug --apk --target x86_64

.PHONY: android-emulator
android-emulator: android ## Start the emulator without a window if needed, install the APK and open it
	scripts/android-emulator.sh

.PHONY: app-sandbox
app-sandbox: ## Try the built app on a pretend computer: its own home, a pretend systemd, port 8187
	mkdir -p $(SANDBOX)/fake-bin $(SANDBOX)/.config/ahab
	cp $(NATIVE_RUST)/tests/fake-systemctl.sh $(SANDBOX)/fake-bin/systemctl
	printf '#!/bin/sh\necho yes\n' > $(SANDBOX)/fake-bin/loginctl
	chmod +x $(SANDBOX)/fake-bin/systemctl $(SANDBOX)/fake-bin/loginctl
	test -f $(SANDBOX)/.config/ahab/config.toml || \
		printf 'listen_addr = "127.0.0.1:$(SANDBOX_PORT)"\n' > $(SANDBOX)/.config/ahab/config.toml
	HOME=$(SANDBOX) XDG_CONFIG_HOME=$(SANDBOX)/.config XDG_DATA_HOME=$(SANDBOX)/.local/share \
		XDG_CACHE_HOME=$(SANDBOX)/.cache XDG_STATE_HOME=$(SANDBOX)/.local/state \
		PATH=$(SANDBOX)/fake-bin:$$(echo "$$PATH" | tr ':' '\n' | while read -r d; do [ -e "$$d/ahab" ] || printf '%s:' "$$d"; done) \
		AHAB_APP_SECRETS=file AHAB_APP_BUNDLED=$(CURDIR)/$(SIDECAR) \
		$(NATIVE_RUST)/target/release/ahab-desktop

.PHONY: app-sandbox-stop
app-sandbox-stop: ## Stop what the sandbox's pretend systemd started
	-HOME=$(SANDBOX) $(SANDBOX)/fake-bin/systemctl --user disable --now ahab
