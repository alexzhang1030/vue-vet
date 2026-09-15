<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
const handle = watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  { immediate: true, flush: 'sync' },
)
handle.pause()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
