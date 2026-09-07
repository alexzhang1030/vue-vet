<script setup lang="ts">
import { ref, watchEffect } from 'vue'
declare const scope: { run: (fn: () => unknown) => unknown; stop: () => void }
const source = ref(0)
await scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
})
scope.stop()
</script>
<template>{{ source }}</template>
