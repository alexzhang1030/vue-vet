<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/shared'
const source = ref(0)
const value = await until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
