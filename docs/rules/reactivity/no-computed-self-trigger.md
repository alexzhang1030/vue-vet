# No Computed Self Trigger

Vue Vet matrix rule `vue-vet/reactivity/no-computed-self-trigger`.

## Bad

See `fixtures/rules/no-computed-self-trigger/invalid/`.

## Good

See `fixtures/rules/no-computed-self-trigger/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
