<script setup lang="ts">
import { ref } from 'vue'
function until(_source: unknown) {
  return { toBe: async () => 0 }
}
const source = ref(0)
const value = await until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
