<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const 源 = ref(0)
const 结果 = await until(源).toBe('ready', { timeout: 5 })
结果.toUpperCase()
</script>

<template>
  <p />
</template>
