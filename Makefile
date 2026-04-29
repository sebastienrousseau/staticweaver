# Makefile using cargo for managing builds and dependencies in a Rust project.

# Default target executed when no arguments are given to make.
.PHONY: all
all: help ## Display this help.

# Build the project including all workspace members.
.PHONY: build
build: ## Build the project.
	@echo "Building all project components..."
	@cargo build --all

# Lint the project with stringent rules using Clippy, install Clippy if not present.
.PHONY: lint
lint: ensure-clippy ## Lint the project with Clippy.
	@echo "Linting with Clippy..."
	@cargo clippy --all-features --all-targets --all -- \
		--deny clippy::dbg_macro --deny clippy::unimplemented --deny clippy::todo --deny warnings \
		--deny missing_docs --deny broken_intra_doc_links --forbid unused_must_use --deny clippy::result_unit_err

# Full test battery — library, integration, doctests, all feature
# combinations, both escape paths, and the remote-templates mock-server
# tests. Matches what the `pre-push` hook runs.
.PHONY: test
test: ## Run tests for the project.
	@echo "Running tests (default features)..."
	@cargo test --all-targets
	@echo "Running tests (--features remote-templates)..."
	@cargo test --all-targets --features remote-templates
	@echo "Running doctests (--all-features)..."
	@cargo test --doc --all-features

# Line-coverage report via cargo-llvm-cov. Installs the binary if
# missing; fails the build if overall line coverage drops below 95%.
.PHONY: coverage
coverage: ## Measure + enforce >= 95% line coverage.
	@command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install --locked cargo-llvm-cov
	@cargo llvm-cov --all-features --summary-only | tee /tmp/staticweaver-cov.txt
	@cov=$$(awk '/^TOTAL/ {for (i=1;i<=NF;i++) if ($$i ~ /%$$/) last=$$i} END {gsub("%","",last); print last}' /tmp/staticweaver-cov.txt); \
		awk -v c="$$cov" 'BEGIN {exit !(c+0 >= 95.0)}' \
			|| { echo "line coverage $$cov% below 95% floor"; exit 1; }

# Check the project for errors without producing outputs.
.PHONY: check
check: ## Check the project for errors without producing outputs.
	@echo "Checking code formatting..."
	@cargo check

# Format all code in the project according to rustfmt's standards, install rustfmt if not present.
.PHONY: format
format: ensure-rustfmt ## Format the code.
	@echo "Formatting all project components..."
	@cargo fmt --all

# Check code formatting without making changes, with verbose output, install rustfmt if not present.
.PHONY: format-check-verbose
format-check-verbose: ensure-rustfmt ## Check code formatting with verbose output.
	@echo "Checking code format with verbose output..."
	@cargo fmt --all -- --check --verbose

# Apply fixes to the project using cargo fix. `cargo fix` ships with the
# toolchain — no component to install.
.PHONY: fix
fix: ## Automatically fix Rust lint warnings using cargo fix.
	@echo "Applying cargo fix..."
	@cargo fix --all

# Use cargo-deny to check for security vulnerabilities, licensing issues, and more, install if not present.
.PHONY: deny
deny: ensure-cargo-deny ## Run cargo deny checks.
	@echo "Running cargo deny checks..."
	@cargo deny check

# Check for outdated dependencies only for the root package, install cargo-outdated if necessary.
.PHONY: outdated
outdated: ensure-cargo-outdated ## Check for outdated dependencies for the root package only.
	@echo "Checking for outdated dependencies..."
	@cargo outdated --root-deps-only

# One-shot bootstrap for a fresh clone. Safe to re-run.
.PHONY: init
init: ## Bootstrap: install git hooks + toolchain components.
	@echo "Pointing git at repo-local hooks..."
	@git config core.hooksPath .githooks
	@chmod +x .githooks/* 2>/dev/null || true
	@echo "Verifying rust toolchain..."
	@cargo --version
	@cargo fmt --version || rustup component add rustfmt
	@cargo clippy --version || rustup component add clippy
	@echo "Done. Run 'make' to list targets."

# Installation checks and setups
.PHONY: ensure-clippy ensure-rustfmt ensure-cargo-deny ensure-cargo-outdated
ensure-clippy:
	@cargo clippy --version || rustup component add clippy

ensure-rustfmt:
	@cargo fmt --version || rustup component add rustfmt

ensure-cargo-deny:
	@command -v cargo-deny || cargo install cargo-deny

ensure-cargo-outdated:
	@command -v cargo-outdated || cargo install cargo-outdated

# Help target to display callable targets and their descriptions.
.PHONY: help
help: ## Display this help.
	@echo "Usage: make [target]..."
	@echo ""
	@echo "Targets:"
	@awk 'BEGIN {FS = ":.*?##"} /^[a-zA-Z_-]+:.*?##/ {printf "  %-30s %s\n", $$1, $$2}' $(MAKEFILE_LIST)