# No On Scope Dispose Reactive Read

Vue Vet matrix rule `vue-vet/reactivity/no-on-scope-dispose-reactive-read`.

## Bad

See `fixtures/rules/no-on-scope-dispose-reactive-read/invalid/`.

## Good

See `fixtures/rules/no-on-scope-dispose-reactive-read/valid/`.

## Detection

Fact-driven via tracking scopes, top-level await call sites, destructures, or operands.
