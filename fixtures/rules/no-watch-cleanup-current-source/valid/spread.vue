<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
const args: [string, () => void] = ['click', handler]
watch(source, (target, _prev, onCleanup) => {
  target.addEventListener(...args)
  onCleanup(() => {
    source.value.removeEventListener(...args)
  })
})
source.value = new EventTarget()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
