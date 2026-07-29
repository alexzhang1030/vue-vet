# No Computed Without Dependency

Vue Vet matrix rule `vue-vet/reactivity/no-computed-without-dependency`.

## Bad

See `fixtures/rules/no-computed-without-dependency/invalid/`.

## Good

See `fixtures/rules/no-computed-without-dependency/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
