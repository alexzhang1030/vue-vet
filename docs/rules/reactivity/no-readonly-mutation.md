# No Readonly Mutation

Vue Vet matrix rule for `no-readonly-mutation`.

## Bad

See `fixtures/rules/no-readonly-mutation/invalid/`.

## Good

See `fixtures/rules/no-readonly-mutation/valid/`.

## Detection

Fact-driven via `vue_vet_reactivity` tracking scopes / script call facts / operands.
