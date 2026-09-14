<script setup lang="ts">
import { shallowRef, watch } from 'vue'
const handler = () => {}
const source = shallowRef(new EventTarget())
watch(source, (target, _prev, onCleanup) => {
  target.addEventListener('click', handler)
  onCleanup(() => {
    source.value.removeEventListener('click', handler)
  })
}, { immediate: true, flush: 'sync' })
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
