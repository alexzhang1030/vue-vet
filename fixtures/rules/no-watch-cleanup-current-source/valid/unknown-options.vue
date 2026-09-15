<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
const options = { immediate: true, flush: 'sync' as const }
watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  options,
)
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
