set shell := ["bash", "-cu"]
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

_default:
  @just --list -u

# Prepare a fresh checkout after `just` itself is installed.
setup:
  rustup component add clippy rustfmt
  cargo install prek --version 0.4.9 --locked
  prek install --hook-type pre-commit --hook-type pre-push
  just doctor

# Verify the pinned Rust toolchain and all repository development tools.
doctor:
  rustc --version --verbose
  cargo --version
  cargo clippy --version
  cargo fmt --version
  just --version
  prek --version
  prek validate-config .pre-commit-config.yaml
  cargo check --workspace --locked --quiet

# Run Vue Vet; pass CLI arguments after the recipe name.
vet *args:
  cargo run -p vue-vet -- {{args}}

# Run the complete Rust validation suite.
roll-rust: lint-rust test

# Refresh committed Vue onTrack oracle fixtures (requires pnpm + Node).
oracle-refresh:
  cd crates/vue-vet-reactivity/oracle && pnpm install && pnpm oracle:write

# Compare static tracer to committed runtime oracle (no Node required).
oracle:
  cargo test -p vue-vet-reactivity --lib oracle --locked

# Run all non-mutating Rust linters.
lint-rust: fmt-check check clippy

# Type-check every workspace crate using the committed lockfile.
check:
  cargo check --workspace --all-targets --all-features --locked

# Format all Rust source files.
fmt:
  cargo fmt --all

# Verify Rust formatting without changing files.
fmt-check:
  cargo fmt --all --check

# Run Clippy with the workspace lint policy and no warnings.
clippy:
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Run all tests using the committed lockfile.
test:
  cargo test --workspace --all-features --locked

# Run the representative SFC analysis benchmarks locally.
bench:
  cargo bench -p vue-vet-vize --bench analyze_sfc --locked

# Build the representative benchmarks with CodSpeed instrumentation.
bench-codspeed-build:
  cargo codspeed build -p vue-vet-vize --bench analyze_sfc --profile codspeed --locked

# Run the most recently built CodSpeed benchmark suite.
bench-codspeed-run:
  cargo codspeed run -p vue-vet-vize --bench analyze_sfc

# Generate an LCOV report for Codecov and local coverage tools.
coverage-lcov:
  cargo llvm-cov --workspace --all-features --locked --lcov --output-path lcov.info

# Run CLI fixture smoke tests only.
smoke:
  cargo test -p vue-vet --test cli --locked

# Run the golden fixture and reporter snapshots in one unified feature build.
snapshots: test

# Apply safe formatter and Clippy fixes to the working tree.
fix-rust:
  cargo fmt --all
  cargo clippy --fix --workspace --all-targets --all-features --allow-dirty --allow-staged

# Run every configured Git hook against the repository.
precommit:
  prek run --all-files

# Install the prek-managed Git hook.
install-hooks:
  prek install
