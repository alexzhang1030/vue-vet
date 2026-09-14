<script setup lang="ts">
import { ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const source = ref(1)
const sink = ref(0)
const value = computedAsync(async () => source.value * 10, -1)
source.value = 3
watch(value, (current) => {
  if (current !== -1) sink.value = current
}, { immediate: true })
</script>

<template>
  <p />
</template>
