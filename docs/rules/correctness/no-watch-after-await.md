# No Watch After Await

Vue Vet matrix rule `vue-vet/correctness/no-watch-after-await`.

## Bad

See `fixtures/rules/no-watch-after-await/invalid/`.

## Good

See `fixtures/rules/no-watch-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
