<script setup lang="ts">
import { ref as useRef, watch as observe } from 'vue'
const handler = () => {}
const source = useRef(new EventTarget())
const alias = source
observe(alias, (target, _prev, onCleanup) => {
  target.addEventListener('click', handler)
  onCleanup(() => {
    source.value.removeEventListener('click', handler)
  })
}, { immediate: true, flush: 'sync' })
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
