# Disallow duplicate `defineProps` calls

This high-confidence rule reports multiple `defineProps` compiler macro calls in one `<script setup>` block.

## Bad

```vue
<script setup lang="ts">
const name = defineProps<{ name: string }>()
const count = defineProps<{ count: number }>()
</script>
```

## Good

```vue
<script setup lang="ts">
const props = defineProps<{ name: string; count: number }>()
</script>
```

## Limitations

The rule applies only to the Vue `<script setup>` compiler macro, not similarly named functions in a normal script block.

## Remediation

Merge every prop declaration into one `defineProps` call.
