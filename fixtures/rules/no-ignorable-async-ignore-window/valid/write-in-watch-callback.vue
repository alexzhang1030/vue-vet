<script setup lang="ts">
import { ref, watch } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = ref(0)
const trigger = ref(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
watch(trigger, () => {
  source.value = 2
}, { flush: 'sync' })
trigger.value = 1
void ignoreUpdates(async () => {
  await Promise.resolve()
  source.value = 2
})
</script>

<template>
  <p />
</template>
