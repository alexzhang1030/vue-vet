# No Define Expose After Await

Vue Vet matrix rule `vue-vet/correctness/no-define-expose-after-await`.

## Bad

See `fixtures/rules/no-define-expose-after-await/invalid/`.

## Good

See `fixtures/rules/no-define-expose-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
