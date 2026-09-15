<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
const flag = Math.random() > 2
if (flag) {
  source.value = 'other'
}
const value = await until(source).toBe(1, { timeout: 5 })
value.toFixed(1)
</script>

<template>
  <p />
</template>
