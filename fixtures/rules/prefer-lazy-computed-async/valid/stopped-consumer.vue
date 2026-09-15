<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const source = ref(1)
const sink = ref(0)
const value = computedAsync(async () => source.value * 10, -1)
source.value = 3
await nextTick()
await nextTick()
const stop = watch(value, (current) => {
  if (current !== -1) sink.value = current
}, { immediate: true })
stop()
</script>

<template>
  <p />
</template>
