<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
try {
  const value = await until(source).toBe('ready', { timeout: 5, throwOnTimeout: true })
  value.toUpperCase()
} catch {
  void 0
}
</script>

<template>
  <p />
</template>
