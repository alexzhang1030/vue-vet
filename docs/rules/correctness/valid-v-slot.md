# Disallow invalid `v-slot` usage

This high-confidence rule reports `v-slot` combined with `v-for` on the same element. `v-for` iterates the whole node, so pairing it with `v-slot` makes the slot scope ambiguous and Vue rejects it at compile time.

## Bad

```vue
<template #item="{ item }" v-for="item in items">
  {{ item }}
</template>
```

## Good

```vue
<template v-for="item in items">
  <template #item="{ item }">
    {{ item }}
  </template>
</template>
```

## Limitations

Detects only `v-slot` and `v-for` co-occurring on the same element. Other invalid `v-slot` placements (for example on a non-component, non-`<template>` element) are not yet covered.

## Remediation

Wrap the `v-for` content in a nested element, or move `v-slot` onto the parent `<template>`.
