<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
watch(source, (target, _prev, onCleanup) => {
  const controller = new AbortController()
  target.addEventListener('click', handler, { signal: controller.signal })
  onCleanup(() => {
    source.value.removeEventListener('click', handler)
  })
})
source.value = new EventTarget()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
