<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const initial = new EventTarget()
const source = ref(initial)
let candidate = initial
candidate = initial
let forwarded = candidate
Reflect.set(forwarded, 'addEventListener', () => {})
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
