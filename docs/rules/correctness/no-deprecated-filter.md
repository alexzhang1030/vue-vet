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

Detects a single spaced pipe (` | `, not `||`) in a template expression. Bitwise-or usage written without surrounding spaces (`a|b`) is not flagged; bitwise-or written with spaces (`a | b`) is a known false-positive risk, though rare in template expressions.

## Remediation

Replace the filter with a plain method call or a `computed` property.
