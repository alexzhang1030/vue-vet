<script setup lang="ts">
import { effectScope, getCurrentScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const retained: Array<{ stop: () => void } | undefined> = []
watch(outer, () => {
  const owner = effectScope(true)
  owner.run(() => {
    if (true) retained.push(getCurrentScope())
    watch(inner, () => {}, { flush: 'sync' })
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
