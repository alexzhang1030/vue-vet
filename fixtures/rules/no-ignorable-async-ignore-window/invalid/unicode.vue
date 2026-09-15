<script setup lang="ts">
import { ref } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const 源 = ref(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(源, (value) => {
  seen.push(value)
}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  源.value = 2
})
</script>

<template>
  <p />
</template>
