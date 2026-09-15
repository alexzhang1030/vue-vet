<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
until(source).toBe('ready', { timeout: 5 }).then((value) => value.toUpperCase())
</script>

<template>
  <p />
</template>
