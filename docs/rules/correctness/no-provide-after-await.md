# No Provide After Await

Vue Vet matrix rule `vue-vet/correctness/no-provide-after-await`.

## Bad

See `fixtures/rules/no-provide-after-await/invalid/`.

## Good

See `fixtures/rules/no-provide-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
