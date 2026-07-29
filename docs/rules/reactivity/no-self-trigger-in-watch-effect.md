# No Self Trigger In Watch Effect

Vue Vet matrix rule `vue-vet/reactivity/no-self-trigger-in-watch-effect`.

## Bad

See `fixtures/rules/no-self-trigger-in-watch-effect/invalid/`.

## Good

See `fixtures/rules/no-self-trigger-in-watch-effect/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
