<script setup lang="ts">
import { ref } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
function watch(
  _source: typeof source,
  callback: (target: EventTarget, prev: unknown, onCleanup: (fn: () => void) => void) => void,
) {
  callback(source.value, undefined, (fn) => fn())
}
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
