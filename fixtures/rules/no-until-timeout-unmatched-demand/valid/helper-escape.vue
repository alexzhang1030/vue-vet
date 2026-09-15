<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
function write(target: { value: unknown }) {
  target.value = 'ready'
}
const source = ref(0)
write(source)
const value = await until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
