# No Watch Effect After Await

Vue Vet matrix rule `vue-vet/correctness/no-watch-effect-after-await`.

## Bad

See `fixtures/rules/no-watch-effect-after-await/invalid/`.

## Good

See `fixtures/rules/no-watch-effect-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
