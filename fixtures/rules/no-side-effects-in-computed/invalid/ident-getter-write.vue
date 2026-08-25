<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const b = ref(0)
function load() {
  b.value = a.value
  return a.value
}
const c = computed(load)
</script>
<template>{{ c }}</template>
