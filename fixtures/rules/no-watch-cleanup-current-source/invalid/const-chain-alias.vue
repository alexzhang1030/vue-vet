<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const initial = new EventTarget()
const source = ref(initial)
const fresh = new EventTarget()
const middle = fresh
const replacement = middle
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
source.value = replacement
</script>
<template>{{ source }}</template>
