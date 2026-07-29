# No Self Trigger In Watch Post Effect

Vue Vet matrix rule `vue-vet/reactivity/no-self-trigger-in-watch-post-effect`.

## Bad

See `fixtures/rules/no-self-trigger-in-watch-post-effect/invalid/`.

## Good

See `fixtures/rules/no-self-trigger-in-watch-post-effect/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
