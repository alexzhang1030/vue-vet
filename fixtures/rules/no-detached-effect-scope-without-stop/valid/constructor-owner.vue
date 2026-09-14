<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
class Resource {
  constructor(readonly scope: ReturnType<typeof effectScope>) {}
  stop() {
    this.scope.stop()
  }
}
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  new Resource(scope)
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
