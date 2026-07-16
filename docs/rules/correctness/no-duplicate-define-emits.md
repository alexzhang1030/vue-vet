# Keep one defineEmits declaration

This high-confidence rule reports more than one `defineEmits` call in the same `<script setup>` block.

## Bad

```vue
defineEmits<{ save: [] }>()
defineEmits<{ cancel: [] }>()
```

## Good

```vue
defineEmits<{ save: []; cancel: [] }>()
```

## Remediation

Merge all emitted-event declarations into one compiler macro call.
