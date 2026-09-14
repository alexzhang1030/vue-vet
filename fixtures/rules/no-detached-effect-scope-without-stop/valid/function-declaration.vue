<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const inner = ref(0)
function listen() {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
}
listen()
</script>
<template>{{ inner }}</template>
