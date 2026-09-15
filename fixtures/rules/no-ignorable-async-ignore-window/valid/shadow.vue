<script setup lang="ts">
import { ref } from 'vue'
const watchIgnorable = (_source: unknown, _cb: unknown, _opts: unknown) => ({
  ignoreUpdates: (fn: () => void) => fn(),
})
const source = ref(0)
const { ignoreUpdates } = watchIgnorable(source, () => {}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  source.value = 2
})
</script>

<template>
  <p />
</template>
