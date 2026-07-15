set shell := ["bash", "-cu"]
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

_default:
  @just --list -u

# Run Vue Vet; pass CLI arguments after the recipe name.
vet *args:
  cargo run -p vue-vet -- {{args}}

# Run the complete Rust validation suite.
roll-rust: lint-rust test

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
