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

# Run Rust validation plus npm launcher tests.
roll: roll-rust npm-test

# Refresh committed Vue onTrack oracle fixtures (requires pnpm + Node).
oracle-refresh:
  cd crates/vue_vet_reactivity/oracle && pnpm install && pnpm oracle:write

# Compare static tracer to committed runtime oracle (no Node required).
oracle:
  cargo test -p vue_vet_reactivity --lib oracle --locked

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

# Run the representative SFC and project scan-mode benchmarks locally.
bench:
  cargo bench -p vue_vet_vize --bench analyze_sfc --locked
  cargo bench -p vue_vet_session --bench scan_modes --locked

# Build the representative benchmarks with CodSpeed instrumentation.
bench-codspeed-build:
  cargo codspeed build -p vue_vet_vize --bench analyze_sfc --profile codspeed --locked
  cargo codspeed build -p vue_vet_session --bench scan_modes --profile codspeed --locked

# Run the most recently built CodSpeed benchmark suite.
bench-codspeed-run:
  cargo codspeed run

# Print quality-corpus tree digests (update fixtures/quality/manifest.json after intentional edits).
quality-digest:
  cargo test -p vue-vet --test quality_gates digest_printer -- --exact --ignored --nocapture

# Integrity, precision expectations, and cold/warm diagnostic identity for issue #13.
quality-gates:
  cargo test -p vue-vet --test quality_gates --locked

# Verify pinned Rust / Vize / Oxc / Vue fixture versions against fixtures/quality/compat-matrix.json.
compat-matrix:
  cargo test -p vue-vet --test compat_matrix --locked

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

# Run npm launcher unit tests (Node >= 18).
npm-test:
  cd npm/vue-vet && npm test

# Sync npm/vue-vet package.json version + optionalDependencies.
npm-sync-version version:
  node npm/scripts/sync-launcher-version.mjs {{version}}

# Pack the host release binary into dist/npm/@vue-vet/<os>-<cpu>.
pack-platform:
  #!/usr/bin/env bash
  set -euo pipefail
  cargo build -p vue-vet --release --locked
  host="$(rustc -vV | sed -n 's/^host: //p')"
  case "$host" in
    x86_64-unknown-linux-gnu) pkg=linux-x64 ;;
    aarch64-unknown-linux-gnu) pkg=linux-arm64 ;;
    x86_64-apple-darwin) pkg=darwin-x64 ;;
    aarch64-apple-darwin) pkg=darwin-arm64 ;;
    x86_64-pc-windows-msvc) pkg=win32-x64 ;;
    *) echo "unsupported host target: $host" >&2; exit 1 ;;
  esac
  version="$(cargo metadata --no-deps --format-version 1 --manifest-path crates/vue_vet_cli/Cargo.toml \
    | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>{const j=JSON.parse(s);const p=j.packages.find(p=>p.name==="vue-vet");if(!p)process.exit(1);process.stdout.write(p.version)})')"
  binary="target/release/vue-vet"
  if [[ "$host" == *windows* ]]; then
    binary="${binary}.exe"
  fi
  node npm/scripts/pack-platform.mjs \
    --target "$host" \
    --binary "$binary" \
    --version "$version" \
    --out "dist/npm/@vue-vet/${pkg}"

# Smoke the host release binary (--version + fixture scan).
release-smoke:
  #!/usr/bin/env bash
  set -euo pipefail
  cargo build -p vue-vet --release --locked
  binary="target/release/vue-vet"
  "$binary" --version
  "$binary" fixtures/projects/basic

# Pack host platform package and smoke the npm launcher against it (no registry).
npm-smoke:
  #!/usr/bin/env bash
  set -euo pipefail
  just pack-platform
  just npm-sync-version 0.1.0
  host="$(rustc -vV | sed -n 's/^host: //p')"
  case "$host" in
    x86_64-unknown-linux-gnu) pkg=linux-x64 ;;
    aarch64-unknown-linux-gnu) pkg=linux-arm64 ;;
    x86_64-apple-darwin) pkg=darwin-x64 ;;
    aarch64-apple-darwin) pkg=darwin-arm64 ;;
    x86_64-pc-windows-msvc) pkg=win32-x64 ;;
    *) echo "unsupported host target: $host" >&2; exit 1 ;;
  esac
  smoke="$(mktemp -d)"
  cleanup() { rm -rf "$smoke"; }
  trap cleanup EXIT
  mkdir -p "$smoke"
  (
    cd "$smoke"
    npm init -y >/dev/null
    npm install --omit=optional "$OLDPWD/dist/npm/@vue-vet/${pkg}"
    npm install --omit=optional "$OLDPWD/npm/vue-vet"
    npx --package=@vue-vet/cli vue-vet --version
    npx --package=@vue-vet/cli vue-vet "$OLDPWD/fixtures/projects/basic"
  )

# Publish host platform package + launcher (requires valid npm auth + @vue-vet org).
npm-publish-host *args:
  node npm/scripts/publish-local-host.mjs {{args}}
