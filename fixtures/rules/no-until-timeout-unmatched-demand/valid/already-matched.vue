<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref('ready')
const value = await until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
