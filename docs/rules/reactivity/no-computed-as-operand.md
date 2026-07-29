# No Computed As Operand

Vue Vet matrix rule `vue-vet/reactivity/no-computed-as-operand`.

## Bad

See `fixtures/rules/no-computed-as-operand/invalid/`.

## Good

See `fixtures/rules/no-computed-as-operand/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
