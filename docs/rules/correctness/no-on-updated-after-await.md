# No On Updated After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-updated-after-await`.

## Bad

See `fixtures/rules/no-on-updated-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-updated-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
