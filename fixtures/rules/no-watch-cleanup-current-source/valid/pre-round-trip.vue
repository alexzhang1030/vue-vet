<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
const handler = () => {}
const initial = new EventTarget()
const source = ref(initial)
const stop = watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  { immediate: true },
)
source.value = new EventTarget()
source.value = initial
await nextTick()
stop()
</script>
<template>{{ source }}</template>
