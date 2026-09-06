<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
function effect() {
  source.value
  return () => {}
}
watchEffect(effect)
</script>
<template>{{ source }}</template>
