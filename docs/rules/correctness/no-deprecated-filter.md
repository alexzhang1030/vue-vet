# Disallow Vue 2 pipe filters

This high-confidence rule reports the legacy `expr | filterName` pipe-filter syntax, which was removed in Vue 3.

## Bad

```vue
<template>
  <p>{{ message | capitalize }}</p>
</template>
```

## Good

```vue
<template>
  <p>{{ capitalize(message) }}</p>
</template>
```

## Limitations

Detects a spaced pipe (` | `, not `||`) whose right-hand side looks like a Vue 2
filter name (`ident` or `ident(...)`). Member / type unions such as
`Foo.Bar | Foo.Baz` and TypeScript assertions stay quiet. Standalone `.jsx` /
`.tsx` modules are skipped — pipe filters are SFC template syntax only.
Bitwise-or written with spaces between two bare identifiers (`a | b`) remains a
known false-positive risk, though rare in templates.

## Remediation

Replace the filter with a plain method call or a `computed` property.
