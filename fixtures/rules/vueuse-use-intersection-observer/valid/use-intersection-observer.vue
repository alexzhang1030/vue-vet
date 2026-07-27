<script setup lang="ts">
import { ref } from 'vue'
import { useIntersectionObserver } from '@vueuse/core'

const el = ref<HTMLElement | null>(null)

useIntersectionObserver(el, () => {
  // visible
})
</script>

<template>
  <div ref="el" />
</template>
