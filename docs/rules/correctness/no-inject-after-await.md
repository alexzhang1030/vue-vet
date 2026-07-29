# No Inject After Await

Vue Vet matrix rule `vue-vet/correctness/no-inject-after-await`.

## Bad

See `fixtures/rules/no-inject-after-await/invalid/`.

## Good

See `fixtures/rules/no-inject-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
