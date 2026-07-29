# No Template Ref Read In Setup

Vue Vet matrix rule for `no-template-ref-read-in-setup`.

## Bad

See `fixtures/rules/no-template-ref-read-in-setup/invalid/`.

## Good

See `fixtures/rules/no-template-ref-read-in-setup/valid/`.

## Detection

Fact-driven via `vue_vet_reactivity` tracking scopes / script call facts / operands.
