<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = null
const source = ref(new EventTarget())
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
source.value = new EventTarget()
stop()
</script>
<template>{{ source }}</template>
