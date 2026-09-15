<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
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
function unusedReplacement() {
  source.value = new EventTarget()
  source.value = new EventTarget()
}
void unusedReplacement
</script>
<template>{{ source }}</template>
