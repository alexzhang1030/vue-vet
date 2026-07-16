# Contributing to Vue Vet

Vue Vet is an Alpha Rust project. Changes should preserve deterministic
diagnostics, exact source spans, and the versioned machine-readable contracts.

## Setup

Install `just`, clone the repository, then run:

```sh
just setup
just roll-rust
```

The repository pins Rust 1.97.0 and the dependency lockfile. Do not update Vize,
Oxc, or serialized format versions without compatibility fixtures and rationale.

## Changes

- Add positive, safe, malformed, and regression fixtures as appropriate.
- Every default rule needs documentation, confidence evidence, and exact-span tests.
- Keep built-in semantic rules authoritative over overlapping custom patterns.
- Run `just precommit` and `just roll-rust` before opening a pull request.
- Use conventional commit messages.

Unexpected diagnostics, snapshot changes, and contract changes must be explained
in the pull request. Security reports must follow [SECURITY.md](SECURITY.md).
