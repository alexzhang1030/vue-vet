<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const source = ref(0)
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  watch(source, () => {})
})
scope.stop()
</script>
<template>{{ source }}</template>
