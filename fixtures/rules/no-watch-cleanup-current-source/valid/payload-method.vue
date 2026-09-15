<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const initial = new EventTarget()
initial.addEventListener = () => {}
const source = ref(initial)
watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  { immediate: true, flush: 'sync' },
)
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
