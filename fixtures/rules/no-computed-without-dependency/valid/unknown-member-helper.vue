<script setup lang="ts">
import { computed } from 'vue'
declare function createState(): { current: Set<string> }
const state = createState()
function load() {
  return state.current.size === 1
}
const active = computed(() => load())
</script>
<template>{{ active }}</template>
