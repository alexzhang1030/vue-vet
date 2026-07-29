# No Self Trigger In Watch Sync Effect

Vue Vet matrix rule `vue-vet/reactivity/no-self-trigger-in-watch-sync-effect`.

## Bad

See `fixtures/rules/no-self-trigger-in-watch-sync-effect/invalid/`.

## Good

See `fixtures/rules/no-self-trigger-in-watch-sync-effect/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
