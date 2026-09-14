<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const owner = effectScope()
owner.run(() => {
  const stop = watch(outer, () => {
    watch(inner, () => {}, { flush: 'sync' })
  }, { immediate: true, flush: 'sync' })
  const snapshot = inner.value
  stop()
  void snapshot
})
</script>
<template>{{ outer }}{{ inner }}</template>
