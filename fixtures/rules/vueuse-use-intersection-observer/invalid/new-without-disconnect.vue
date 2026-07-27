<script setup lang="ts">
import { onMounted, ref } from 'vue'

const el = ref<HTMLElement | null>(null)

onMounted(() => {
  const observer = new IntersectionObserver(() => {
    // visible
  })
  if (el.value) {
    observer.observe(el.value)
  }
})
</script>

<template>
  <div ref="el" />
</template>
