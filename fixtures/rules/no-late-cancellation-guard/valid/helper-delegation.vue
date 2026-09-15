<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
function register(onCleanup: (fn: () => void) => void, flag: { cancelled: boolean }) {
  onCleanup(() => {
    flag.cancelled = true
  })
}
watch(source, async (value, _previous, onCleanup) => {
  const data = await Promise.resolve(value)
  const flag = { cancelled: false }
  register(onCleanup, flag)
  if (!flag.cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
