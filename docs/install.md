# Installing Vue Vet

Vue Vet ships as a native Rust CLI. The recommended install path is the thin
npm launcher, which selects a prebuilt binary for your platform. Analysis logic
never runs in JavaScript.

## npm / pnpm / yarn / bun

```bash
npm install -D @vue-vet/cli
# pnpm add -D @vue-vet/cli
# yarn add -D @vue-vet/cli
# bun add -d @vue-vet/cli
```

```bash
npx vue-vet .
pnpm exec vue-vet .
```

The `@vue-vet/cli` package declares `@vue-vet/{os}-{arch}` packages as
`optionalDependencies`. Your package manager installs only the host platform
package. The launcher resolves that package and forwards argv, stdio, signals,
and exit codes. The CLI binary name remains `vue-vet`.

### Supported platforms (initial matrix)

| npm package | Rust target |
| --- | --- |
| `@vue-vet/linux-x64` | `x86_64-unknown-linux-gnu` |
| `@vue-vet/linux-arm64` | `aarch64-unknown-linux-gnu` |
| `@vue-vet/darwin-x64` | `x86_64-apple-darwin` |
| `@vue-vet/darwin-arm64` | `aarch64-apple-darwin` |
| `@vue-vet/win32-x64` | `x86_64-pc-windows-msvc` |

Unsupported hosts get a clear error and a pointer to the source-build path.

### Optional-dependency install glitches

Some npm versions mishandle optional dependencies. If the launcher reports a
missing `@vue-vet/*` package:

1. Upgrade npm (or use pnpm).
2. Remove `node_modules` and the lockfile entry for `vue-vet`, then reinstall.
3. Or install the host platform package explicitly, for example
   `npm install -D @vue-vet/darwin-arm64`.

## Continuous preview packages (pkg.pr.new)

In-repo pull requests and pushes to `main` that touch crates / npm / lockfiles
build the npm matrix and publish preview tarballs via
[pkg.pr.new](https://github.com/stackblitz-labs/pkg.pr.new)
(workflow [`.github/workflows/pkg-pr-new.yml`](../.github/workflows/pkg-pr-new.yml)).
Docs-only changes are skipped. Nothing is published to the public npm registry.

After the GitHub App
[pkg-pr-new](https://github.com/apps/pkg-pr-new) is installed on the
repository, each PR gets a bot comment with install commands. Typical usage:

```bash
# CLI preview (rewrites optional platform deps to matching preview URLs)
npx https://pkg.pr.new/@vue-vet/cli@<pr-or-sha>
```

Preview versions use `0.0.0-preview-<sha>` so they cannot collide with a later
semver publish of the same workspace version. Branch tips on `main` also get a
`@main` alias when the workflow runs there.

## GitHub Release binaries

Each tagged release (`vX.Y.Z`) publishes archives named
`vue-vet-<rust-target>.tar.gz` (Windows: `.zip`) plus `SHA256SUMS`.

```bash
# Example for Apple Silicon
curl -fsSL -o vue-vet.tar.gz \
  https://github.com/alexzhang1030/vue-vet/releases/download/v0.1.0/vue-vet-aarch64-apple-darwin.tar.gz
tar -xzf vue-vet.tar.gz
./vue-vet --version
```

Verify checksums against `SHA256SUMS` from the same release before running a
downloaded binary in CI.

## Build from source

Requires the pinned Rust toolchain from `rust-toolchain.toml` and `just`.

```bash
git clone https://github.com/alexzhang1030/vue-vet.git
cd vue-vet
just setup
cargo build -p vue-vet --release --locked
./target/release/vue-vet --version
```

Local packaging helpers (used by the release workflow):

```bash
just npm-test
just pack-platform   # packs the host release binary into dist/npm/@vue-vet/...
just release-smoke   # --version + fixture scan with the host release binary
```

## Version alignment

| Surface | Version source |
| --- | --- |
| Cargo workspace | `[workspace.package].version` |
| npm `vue-vet` and `@vue-vet/*` | same semver |
| Git tag / GitHub Release | `v` + semver |

Do not publish mismatched versions across these surfaces.

## Release and rollback

1. Ensure CI is green on `main`.
2. Bump workspace + npm versions together when cutting a release.
3. Push tag `vX.Y.Z` (or run the Release workflow via `workflow_dispatch`).
4. The workflow runs quality gates, publishes library crates to crates.io in
   dependency order (`vue_vet_core` → `vue_vet_reactivity` →
   `vue_vet_plugins`), builds every matrix target, writes `SHA256SUMS`, creates
   the GitHub Release, publishes `@vue-vet/*` platform packages, then publishes
   `@vue-vet/cli`.
5. After a non-dry-run publish, the Release workflow’s `npm-install-smoke` job
   installs `@vue-vet/cli@X.Y.Z` from the public registry on Linux, macOS, and
   Windows (`ubuntu-latest`, `macos-15`, `windows-latest`). It checks
   `vue-vet --version` equals `vue-vet X.Y.Z` and scans
   `fixtures/projects/basic --no-cache --format json` (asserts `ok`,
   `tool.version`, and `diagnostics[].rule_id === vue-vet/security/no-v-html`).
   Bounded retries wait for npm visibility. Manual smoke:
   `npx --yes --package=@vue-vet/cli@X.Y.Z vue-vet --version`.

**Rollback:** yank a bad npm version with
`npm unpublish @vue-vet/cli@X.Y.Z` only within the npm unpublish window;
otherwise publish a fixed `X.Y.Z+1` and mark the GitHub Release as deprecated
in the release notes. Prefer forward fixes over deleting artifacts consumers
may have cached.

**Failed mid-publish:** platform packages may exist without the launcher (or
the reverse), and crates.io may already have `vue_vet_core` /
`vue_vet_reactivity` / `vue_vet_plugins` at that version. Re-run after fixing
the failure; npm and crates.io both reject re-uploads of the same version, so
bump the patch version if a partial publish already succeeded.

## Secrets and first publish checklist

1. Create the npm organization [`@vue-vet`](https://www.npmjs.com/org/create)
   (CLI cannot create orgs).
2. Log in with a granular access token that can publish `@vue-vet/*`
   (`npm login`), or set `NPM_TOKEN` in the environment. Prefer npm Trusted
   Publishing (OIDC) over long-lived write tokens for CI.
3. Add repository secret `NPM_TOKEN` for the Release workflow (until Trusted
   Publishing is configured for every package).
4. Create a crates.io API token at
   [crates.io/settings/tokens](https://crates.io/settings/tokens) with
   publish rights for `vue_vet_core`, `vue_vet_reactivity`, and
   `vue_vet_plugins` (new + update). Add it as repository secret
   **`CARGO_REGISTRY_TOKEN`**. The Release workflow uses it only for non-dry-run
   tag / `workflow_dispatch` publishes.

### Library crates (crates.io)

| Crate | Role | Depends on |
| --- | --- | --- |
| [`vue_vet_core`](https://crates.io/crates/vue_vet_core) | Diagnostics, spans, fact contracts | — |
| [`vue_vet_reactivity`](https://crates.io/crates/vue_vet_reactivity) | Tracer engine (empty ecosystem catalog by default) | `vue_vet_core` |
| [`vue_vet_plugins`](https://crates.io/crates/vue_vet_plugins) | Nuxt / vue-i18n named API bags | `vue_vet_core`, `vue_vet_reactivity` |

Publish order is fixed by dependencies. Product entry points (Oxc adapter,
project graph, session) **auto-load** `vue_vet_plugins` defaults; see
[vue_vet_plugins README](../crates/vue_vet_plugins/README.md). Full workspace
crate map: [docs/crates.md](./crates.md).
5. Local host-only claim (optional before the full matrix release):

   ```bash
   just pack-platform
   just npm-publish-host          # or: just npm-publish-host --dry-run
   just npm-smoke                 # file: install without registry
   ```

6. Full matrix: push tag `v0.1.0` (or run Release via `workflow_dispatch` with
   `dry_run=false`). The tag version must equal `[workspace.package].version`.

GitHub Releases use `GITHUB_TOKEN`. npm provenance uses OIDC (`id-token: write`).

**Note:** publish paths must be absolute or start with `./`. Passing `npm/vue-vet`
to `npm publish` is treated as the git host `github.com/npm/vue-vet`. The
launcher package name is `@vue-vet/cli` (not unscoped `vue-vet`) so it stays
inside the `@vue-vet` org permission boundary.
