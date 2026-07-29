# No Route Destructure

Vue Vet matrix rule `vue-vet/reactivity/no-route-destructure`.

## Bad

See `fixtures/rules/no-route-destructure/invalid/`.

## Good

See `fixtures/rules/no-route-destructure/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
