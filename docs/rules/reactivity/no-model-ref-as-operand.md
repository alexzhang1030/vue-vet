# No Model Ref As Operand

Vue Vet matrix rule `vue-vet/reactivity/no-model-ref-as-operand`.

## Bad

See `fixtures/rules/no-model-ref-as-operand/invalid/`.

## Good

See `fixtures/rules/no-model-ref-as-operand/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
