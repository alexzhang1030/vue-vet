# No On Deactivated After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-deactivated-after-await`.

## Bad

See `fixtures/rules/no-on-deactivated-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-deactivated-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
