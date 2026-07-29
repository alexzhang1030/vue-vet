# No Side Effects In Computed

Vue Vet matrix rule `vue-vet/reactivity/no-side-effects-in-computed`.

## Bad

See `fixtures/rules/no-side-effects-in-computed/invalid/`.

## Good

See `fixtures/rules/no-side-effects-in-computed/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
