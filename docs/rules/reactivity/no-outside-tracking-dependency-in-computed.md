# No Outside Tracking Dependency In Computed

Vue Vet matrix rule `vue-vet/reactivity/no-outside-tracking-dependency-in-computed`.

## Bad

See `fixtures/rules/no-outside-tracking-dependency-in-computed/invalid/`.

## Good

See `fixtures/rules/no-outside-tracking-dependency-in-computed/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
