<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(new EventTarget())
watch(source, (target, _prev, onCleanup) => {
  void target
  onCleanup(() => {
    void source.value
  })
})
source.value = new EventTarget()
source.value = new EventTarget()
</script>
<template>{{ source }}</template>
