# No Assignment Only Effect With Conditional Read

Vue Vet matrix rule `vue-vet/reactivity/no-assignment-only-effect-with-conditional-read`.

## Bad

See `fixtures/rules/no-assignment-only-effect-with-conditional-read/invalid/`.

## Good

See `fixtures/rules/no-assignment-only-effect-with-conditional-read/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
