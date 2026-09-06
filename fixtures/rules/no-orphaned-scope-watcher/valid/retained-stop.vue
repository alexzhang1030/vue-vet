<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
let stop
await scope.run(async () => {
  await Promise.resolve()
  stop = watchEffect(() => {
    source.value
  })
})
scope.stop()
stop?.()
</script>
<template>{{ source }}</template>
