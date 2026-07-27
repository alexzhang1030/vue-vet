<script setup lang="ts">
import { ref } from 'vue'
import { useResizeObserver } from '@vueuse/core'

const el = ref<HTMLElement | null>(null)

useResizeObserver(el, () => {
  // size
})
</script>

<template>
  <div ref="el" />
</template>
