<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
declare function html(strings: TemplateStringsArray, ...values: unknown[]): string
const initial = new EventTarget()
void html`${initial}`
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
