# @vue-vet/cli

Thin npm launcher for the [Vue Vet](https://github.com/alexzhang1030/vue-vet)
native CLI. This package selects the correct `@vue-vet/{os}-{arch}` optional
dependency and forwards arguments, signals, and exit codes. It contains **no**
analysis logic. The installed binary name remains `vue-vet`.

## Install

```bash
npm install -D @vue-vet/cli
# or: pnpm add -D @vue-vet/cli
```

```bash
npx vue-vet .
```

## Supported platforms

| Package | Target |
| --- | --- |
| `@vue-vet/linux-x64` | `x86_64-unknown-linux-gnu` |
| `@vue-vet/linux-arm64` | `aarch64-unknown-linux-gnu` |
| `@vue-vet/darwin-x64` | `x86_64-apple-darwin` |
| `@vue-vet/darwin-arm64` | `aarch64-apple-darwin` |
| `@vue-vet/win32-x64` | `x86_64-pc-windows-msvc` |

See [install docs](../../docs/install.md) for source builds, checksums, and
rollback.

## License

MIT
