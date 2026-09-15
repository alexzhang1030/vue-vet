<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
})
scope.stop()
</script>
<template>
  <p>中文前缀</p>{{ source }}</template>
