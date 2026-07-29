# No On Error Captured After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-error-captured-after-await`.

## Bad

See `fixtures/rules/no-on-error-captured-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-error-captured-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
