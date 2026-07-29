# No Conditional Dependency In Watch Sources

Vue Vet matrix rule `vue-vet/reactivity/no-conditional-dependency-in-watch-sources`.

## Bad

See `fixtures/rules/no-conditional-dependency-in-watch-sources/invalid/`.

## Good

See `fixtures/rules/no-conditional-dependency-in-watch-sources/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
