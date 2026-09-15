<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
const first = await until(source).toBe('ready', { timeout: 5 })
source.value = 'ready'
const second = await until(source).toBe('ready', { timeout: 5 })
second.toUpperCase()
first.toUpperCase()
</script>

<template>
  <p />
</template>
