# No On Mounted After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-mounted-after-await`.

## Bad

See `fixtures/rules/no-on-mounted-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-mounted-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
