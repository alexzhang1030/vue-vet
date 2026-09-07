<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
const immediate = { then(fn: () => void) { fn() } }
watchEffect(() => {
  source.value
  immediate.then(() => {
    onWatcherCleanup(() => {})
  })
})
</script>
<template>{{ source }}</template>
