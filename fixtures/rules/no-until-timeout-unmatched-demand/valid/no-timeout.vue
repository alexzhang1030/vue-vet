<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
source.value = 'ready'
const value = await until(source).toBe('ready')
value.toUpperCase()
</script>

<template>
  <p />
</template>
