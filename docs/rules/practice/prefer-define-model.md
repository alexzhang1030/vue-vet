# Prefer `defineModel` over manual `modelValue` prop/emit

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Declaring a `modelValue` prop alongside `defineEmits` and hand-rolled `emit('update:modelValue', …)` calls duplicates what Vue 3.4+ `defineModel()` does in one call, returning a writable ref that reads the prop and emits the update automatically.

## Bad

```vue
<script setup lang="ts">
const props = defineProps<{ modelValue: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
</script>
```

## Good

```vue
<script setup lang="ts">
const model = defineModel<string>()
</script>
```

## Limitations

Fires when a block calls both `defineProps` and `defineEmits`, has a script binding literally named `modelValue` (for example from destructuring `defineProps`), and does not already call `defineModel`. Requires Vue 3.4 or newer in the resolved environment.

## Remediation

Replace the prop/emit pair with `const model = defineModel()` (or a named model for `v-model:foo`).
