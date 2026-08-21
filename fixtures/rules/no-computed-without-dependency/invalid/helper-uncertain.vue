<script setup lang="ts">
import { computed } from 'vue'
declare function useMediaQuery(q: string): { value: boolean }
const isCoarse = useMediaQuery('(pointer: coarse)')
function load() {
  return isCoarse.value
}
const hint = computed(() => load())
</script>

<template>
  <p>{{ hint }}</p>
</template>
