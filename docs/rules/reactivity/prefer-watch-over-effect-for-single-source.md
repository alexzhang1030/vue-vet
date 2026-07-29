# Prefer Watch Over Effect For Single Source

Vue Vet matrix rule `vue-vet/reactivity/prefer-watch-over-effect-for-single-source`.

## Bad

See `fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/`.

## Good

See `fixtures/rules/prefer-watch-over-effect-for-single-source/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
