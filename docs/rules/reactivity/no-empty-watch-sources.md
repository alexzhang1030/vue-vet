# No Empty Watch Sources

Vue Vet matrix rule `vue-vet/reactivity/no-empty-watch-sources`.

## Bad

See `fixtures/rules/no-empty-watch-sources/invalid/`.

## Good

See `fixtures/rules/no-empty-watch-sources/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
