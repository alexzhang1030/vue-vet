<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
declare function own(scope: ReturnType<typeof effectScope>): void
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  own(scope)
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
