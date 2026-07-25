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
4. The workflow builds every matrix target, writes `SHA256SUMS`, creates the
   GitHub Release, publishes `@vue-vet/*` platform packages, then publishes
   `@vue-vet/cli`.
5. Smoke-install with `npx --package=@vue-vet/cli@X.Y.Z vue-vet --version` on
   at least one Linux, macOS, and Windows host.

**Rollback:** yank a bad npm version with
`npm unpublish @vue-vet/cli@X.Y.Z` only within the npm unpublish window;
otherwise publish a fixed `X.Y.Z+1` and mark the GitHub Release as deprecated
in the release notes. Prefer forward fixes over deleting artifacts consumers
may have cached.

**Failed mid-publish:** platform packages may exist without the launcher (or
the reverse). Re-run the Release workflow after fixing the failure; npm rejects
re-uploads of the same version, so bump the patch version if a partial publish
already succeeded.

## Secrets and first publish checklist

1. Create the npm organization [`@vue-vet`](https://www.npmjs.com/org/create)
   (CLI cannot create orgs).
2. Log in with a granular access token that can publish `@vue-vet/*`
   (`npm login`), or set `NPM_TOKEN` in the environment. Prefer npm Trusted
   Publishing (OIDC) over long-lived write tokens for CI.
3. Add repository secret `NPM_TOKEN` for the Release workflow (until Trusted
   Publishing is configured for every package).
4. Local host-only claim (optional before the full matrix release):

   ```bash
   just pack-platform
   just npm-publish-host          # or: just npm-publish-host --dry-run
   just npm-smoke                 # file: install without registry
   ```

5. Full matrix: push tag `v0.1.0` (or run Release via `workflow_dispatch` with
   `dry_run=false`).

GitHub Releases use `GITHUB_TOKEN`. npm provenance uses OIDC (`id-token: write`).

**Note:** publish paths must be absolute or start with `./`. Passing `npm/vue-vet`
to `npm publish` is treated as the git host `github.com/npm/vue-vet`. The
launcher package name is `@vue-vet/cli` (not unscoped `vue-vet`) so it stays
inside the `@vue-vet` org permission boundary.
