<script setup lang="ts">
import { ref, watch } from 'vue'
const handler = () => {}
const source = ref(new EventTarget())
let current: EventTarget | undefined
watch(source, (target) => {
  current?.removeEventListener('click', handler)
  current = target
  current.addEventListener('click', handler)
})
source.value = new EventTarget()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
