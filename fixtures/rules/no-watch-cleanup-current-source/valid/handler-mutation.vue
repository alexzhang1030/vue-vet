<script setup lang="ts">
import { ref, watch } from 'vue'
let handler = () => {}
handler = () => {}
const source = ref(new EventTarget())
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
