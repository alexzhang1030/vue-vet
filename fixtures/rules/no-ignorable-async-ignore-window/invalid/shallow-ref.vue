<script setup lang="ts">
import { shallowRef } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = shallowRef(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(source, (value) => {
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
