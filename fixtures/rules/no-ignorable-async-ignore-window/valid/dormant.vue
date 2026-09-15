<script setup lang="ts">
import { ref } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = ref(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  const later = () => {
    source.value = 2
  }
  void later
})
</script>

<template>
  <p />
</template>
