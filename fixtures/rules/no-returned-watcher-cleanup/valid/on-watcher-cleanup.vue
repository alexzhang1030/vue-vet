<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(() => {
  source.value
  onWatcherCleanup(() => {})
})
</script>
<template>{{ source }}</template>
