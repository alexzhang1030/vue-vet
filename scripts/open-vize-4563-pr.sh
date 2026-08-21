#!/usr/bin/env bash
# Apply docs/upstream/vize-4563-atelier-sfc-compile.patch on a Vize fork and
# open the upstream PR. Needs a GitHub identity that can fork ubugeeei-prod/vize
# (the Cursor App installation on vue-vet cannot).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
patch="$root/docs/upstream/vize-4563-atelier-sfc-compile.patch"
token="${VIZE_UPSTREAM_GITHUB_TOKEN:-${GH_TOKEN:-${GITHUB_TOKEN:-}}}"
work="${TMPDIR:-/tmp}/vize-4563-pr"
branch="feat/atelier-sfc-compile-feature"

if [[ ! -f "$patch" ]]; then
  echo "missing $patch" >&2
  exit 1
fi
if [[ -z "$token" ]]; then
  echo "set VIZE_UPSTREAM_GITHUB_TOKEN (or GH_TOKEN) to an account that can fork vize" >&2
  exit 1
fi

export GH_TOKEN="$token"
export GITHUB_TOKEN="$token"

login="$(gh api user --jq .login)"
gh repo fork ubugeeei-prod/vize --clone=false --default-branch-only --remote=false >/dev/null
rm -rf "$work"
gh repo clone "$login/vize" "$work" -- --depth=1
git -C "$work" checkout -B "$branch"
git -C "$work" am --3way "$patch"
git -C "$work" push -u origin "$branch"
gh pr create --repo ubugeeei-prod/vize \
  --head "$login:$branch" \
  --base main \
  --title "feat(atelier-sfc): add compile feature for parse-only consumers" \
  --body "Closes ubugeeei-prod/vize#4563.

Parse-only consumers (linters that call \`parse_sfc\` and never emit render functions) can disable default features and skip DOM / Vapor / SSR plus this crate's oxc transform/codegen. \`native\` stays the published default.

Verification on Vize \`main\` @ 6f8a249:

\`\`\`text
cargo clippy -p vize_atelier_sfc --no-default-features -- -D warnings
cargo clippy -p vize_atelier_sfc -- -D warnings
cargo test -p vize_atelier_sfc --no-default-features --lib -- test_parse template_compile_options
cargo test -p vize_atelier_sfc --no-default-features --test sfc_block_boundaries
cargo test -p vize_atelier_sfc --lib -- test_parse dialect_threads test_compile_sfc_with_define_emits compile_sfc_attaches
cargo check -p vize_atelier_jsx
\`\`\`

Consumer: https://github.com/alexzhang1030/vue-vet
Dashmap pin is a separate request: ubugeeei-prod/vize#4564."
