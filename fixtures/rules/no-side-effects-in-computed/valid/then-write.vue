<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const t = ref(0)
const c = computed(() => {
  Promise.resolve().then(() => { t.value = 1 })
  return a.value
})
</script>
<template>{{ c }}</template>
