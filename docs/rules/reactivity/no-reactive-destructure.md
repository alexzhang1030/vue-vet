# No Reactive Destructure

Vue Vet matrix rule `vue-vet/reactivity/no-reactive-destructure`.

## Bad

See `fixtures/rules/no-reactive-destructure/invalid/`.

## Good

See `fixtures/rules/no-reactive-destructure/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
