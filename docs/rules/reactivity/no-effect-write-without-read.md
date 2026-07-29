# No Effect Write Without Read

Vue Vet matrix rule `vue-vet/reactivity/no-effect-write-without-read`.

## Bad

See `fixtures/rules/no-effect-write-without-read/invalid/`.

## Good

See `fixtures/rules/no-effect-write-without-read/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
