<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  const scope = effectScope(true)
  let stop: (() => void) | undefined
  scope.run(() => {
    stop = watch(inner, () => {}, { flush: 'sync' })
  })
  stop?.()
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
