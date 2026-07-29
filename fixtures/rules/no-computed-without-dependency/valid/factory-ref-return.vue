<script setup lang="ts">
import { computed, ref } from 'vue'

function useFlag() {
  const flag = ref(false)
  return flag
}

const isCoarsePointer = useFlag()
const hint = computed(() =>
  isCoarsePointer.value
    ? 'drag to pan · pinch to zoom'
    : '⌘/Ctrl + scroll to zoom · middle-drag to pan',
)
</script>

<template>
  <p>{{ hint }}</p>
</template>
