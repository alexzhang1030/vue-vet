<script setup lang="ts">
import { ref } from 'vue'
import { ignorableWatch } from '@vueuse/core'
const source = ref(0)
const seen: number[] = []
const { ignoreUpdates } = ignorableWatch(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  source.value = 2
})
</script>

<template>
  <p />
</template>
