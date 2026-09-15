<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const expected = 'ready'
const source = ref(0)
const value = await until(source).toBe(expected, { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
