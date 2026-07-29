# Prefer Store To Refs

Vue Vet matrix rule `vue-vet/reactivity/prefer-store-to-refs`.

## Bad

See `fixtures/rules/prefer-store-to-refs/invalid/`.

## Good

See `fixtures/rules/prefer-store-to-refs/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
