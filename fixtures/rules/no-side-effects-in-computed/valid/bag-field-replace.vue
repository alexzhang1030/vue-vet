<script setup lang="ts">
import { computed, ref } from 'vue'
function useX() {
  const field = ref(0)
  return { field }
}
const a = ref(0)
const bag = useX()
const c = computed(() => { bag.field = a; return a.value })
</script>
<template>{{ c }}</template>
