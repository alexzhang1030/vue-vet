# No After Await Dependency In Computed

Vue Vet matrix rule `vue-vet/reactivity/no-after-await-dependency-in-computed`.

## Bad

See `fixtures/rules/no-after-await-dependency-in-computed/invalid/`.

## Good

See `fixtures/rules/no-after-await-dependency-in-computed/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
