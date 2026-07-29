# No On Before Update After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-before-update-after-await`.

## Bad

See `fixtures/rules/no-on-before-update-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-before-update-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
