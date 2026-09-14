<script setup lang="ts">
import { effectScope, getCurrentScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const scopes: Array<ReturnType<typeof effectScope>> = []
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    scopes.push(getCurrentScope())
    watch(inner, () => {}, { flush: 'sync' })
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
