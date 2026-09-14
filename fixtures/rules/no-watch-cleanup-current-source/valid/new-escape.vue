<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
class Wrap {
  constructor(public target: EventTarget) {}
}
const initial = new EventTarget()
void new Wrap(initial)
const source = ref(initial)
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
