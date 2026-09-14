<script setup lang="ts">
import { effectScope, ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watchEffect(() => {
      inner.value
    })
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
