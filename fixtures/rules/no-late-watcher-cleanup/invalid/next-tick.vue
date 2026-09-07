<script setup lang="ts">
import { nextTick, onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(() => {
  source.value
  nextTick(() => {
    onWatcherCleanup(() => {})
  })
})
</script>
<template>{{ source }}</template>
