# No On Unmounted After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-unmounted-after-await`.

## Bad

See `fixtures/rules/no-on-unmounted-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-unmounted-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
