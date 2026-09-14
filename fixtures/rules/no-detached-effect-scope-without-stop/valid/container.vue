<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const leaked: Array<ReturnType<typeof effectScope>> = []
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  leaked.push(scope)
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
