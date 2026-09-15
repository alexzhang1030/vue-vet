<script setup lang="ts">
import { ref } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = ref(0)
const seen: number[] = []
const { ignoreUpdates, stop } = watchIgnorable(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
stop()
void ignoreUpdates(async () => {
  await Promise.resolve()
  source.value = 2
})
</script>

<template>
  <p />
</template>
