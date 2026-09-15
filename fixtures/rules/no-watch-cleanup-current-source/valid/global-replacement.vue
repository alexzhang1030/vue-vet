<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
;(globalThis as typeof globalThis & { EventTarget: typeof EventTarget }).EventTarget = EventTarget
watch(source, (target, _prev, onCleanup) => {
  target.addEventListener('click', handler)
  onCleanup(() => {
    source.value.removeEventListener('click', handler)
  })
})
source.value = new EventTarget()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
