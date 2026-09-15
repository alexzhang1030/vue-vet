<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const initial = new EventTarget()
const source = ref(initial)
let replacement = initial
if (false) replacement = new EventTarget()
const stop = watch(
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
stop()
</script>
<template>{{ source }}</template>
