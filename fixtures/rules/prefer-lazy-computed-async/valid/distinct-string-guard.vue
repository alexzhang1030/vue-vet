<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const source = ref(1)
const sink = ref(0)
const value = computedAsync(async () => source.value * 10, 'loading')
source.value = 3
await nextTick()
await nextTick()
watch(value, (current) => {
  if (current !== 'missing') sink.value = current
}, { immediate: true })
</script>

<template>
  <p />
</template>
