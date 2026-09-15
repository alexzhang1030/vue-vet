<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(true)
const pending = until(source).toBe(1, { timeout: 5 })
source.value++
const value = await pending
value.toFixed(1)
</script>

<template>
  <p />
</template>
