# No Router Destructure

Vue Vet matrix rule `vue-vet/reactivity/no-router-destructure`.

## Bad

See `fixtures/rules/no-router-destructure/invalid/`.

## Good

See `fixtures/rules/no-router-destructure/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
