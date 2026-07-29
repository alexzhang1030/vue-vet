# No Shallow Reactive Destructure

Vue Vet matrix rule `vue-vet/reactivity/no-shallow-reactive-destructure`.

## Bad

See `fixtures/rules/no-shallow-reactive-destructure/invalid/`.

## Good

See `fixtures/rules/no-shallow-reactive-destructure/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
