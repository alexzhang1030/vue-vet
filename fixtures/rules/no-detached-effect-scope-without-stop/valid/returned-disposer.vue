<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const inner = ref(0)
function listen() {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  return () => scope.stop()
}
const dispose = listen()
dispose()
</script>
<template>{{ inner }}</template>
