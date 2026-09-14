<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
function tag(_strings: TemplateStringsArray, value: unknown) {
  void value
}
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  tag`${scope}`
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
