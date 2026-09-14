<script setup lang="ts">
import { onWatcherCleanup, ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
watch(source, (target) => {
  target.addEventListener('click', handler)
  onWatcherCleanup(() => {
    source.value.removeEventListener('click', handler)
  })
}, { immediate: true, flush: 'sync' })
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
