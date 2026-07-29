# No On Render Tracked After Await

Vue Vet matrix rule `vue-vet/correctness/no-on-render-tracked-after-await`.

## Bad

See `fixtures/rules/no-on-render-tracked-after-await/invalid/`.

## Good

See `fixtures/rules/no-on-render-tracked-after-await/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
