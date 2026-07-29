# No On Server Prefetch After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-server-prefetch-after-await`.

## Bad

See `fixtures/rules/no-on-server-prefetch-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-server-prefetch-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
