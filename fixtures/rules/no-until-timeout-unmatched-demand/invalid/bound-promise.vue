<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
const pending = until(source).toBe('ready', { timeout: 5 })
const value = await pending
value.toUpperCase()
</script>

<template>
  <p />
</template>
