<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
const key = 'addEventListener'
watch(
  source,
  (target, _prev, onCleanup) => {
    ;(target as unknown as Record<string, unknown>)[key] = () => {}
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
