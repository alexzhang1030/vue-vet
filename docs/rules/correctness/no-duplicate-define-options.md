# Keep one defineOptions declaration

This high-confidence rule reports multiple `defineOptions` calls in one `<script setup>` block.

## Bad

```vue
defineOptions({ name: 'Panel' })
defineOptions({ inheritAttrs: false })
```

## Good

```vue
defineOptions({ name: 'Panel', inheritAttrs: false })
```

## Remediation

Merge component options into one compiler macro call.
