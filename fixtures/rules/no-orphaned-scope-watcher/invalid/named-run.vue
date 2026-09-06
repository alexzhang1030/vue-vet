<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
async function task() {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
}
scope.run(task)
scope.stop()
</script>
<template>{{ source }}</template>
