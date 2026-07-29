# No Watch Callback As Tracking Scope

Vue Vet matrix rule for `no-watch-callback-as-tracking-scope`.

## Bad

See `fixtures/rules/no-watch-callback-as-tracking-scope/invalid/`.

## Good

See `fixtures/rules/no-watch-callback-as-tracking-scope/valid/`.

## Detection

Fact-driven via `vue_vet_reactivity` tracking scopes / script call facts / operands.
