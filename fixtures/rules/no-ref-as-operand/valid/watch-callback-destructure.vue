<script setup lang="ts">
import { ref, watch } from 'vue'

const values = ref([1, 2, 3])
const element = ref<HTMLElement | null>(null)
watch([values, element], ([values, element]) => {
  if (!element || !values.length)
    return
})
</script>

<template>
  <div ref="element" />
</template>
