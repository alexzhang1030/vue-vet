<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const ms = 5
const source = ref(0)
const value = await until(source).toBe('ready', { timeout: ms })
value.toUpperCase()
</script>

<template>
  <p />
</template>
