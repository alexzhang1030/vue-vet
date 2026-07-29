# No On Before Mount After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-before-mount-after-await`.

## Bad

See `fixtures/rules/no-on-before-mount-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-before-mount-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
