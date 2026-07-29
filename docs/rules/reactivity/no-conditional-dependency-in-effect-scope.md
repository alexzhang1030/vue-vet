# No Conditional Dependency In Effect Scope

Vue Vet matrix rule `vue-vet/reactivity/no-conditional-dependency-in-effect-scope`.

## Bad

See `fixtures/rules/no-conditional-dependency-in-effect-scope/invalid/`.

## Good

See `fixtures/rules/no-conditional-dependency-in-effect-scope/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
