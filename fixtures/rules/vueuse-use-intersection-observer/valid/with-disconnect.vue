<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'

const el = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

onMounted(() => {
  observer = new IntersectionObserver(() => {
    // visible
  })
  if (el.value) {
    observer.observe(el.value)
  }
})

onBeforeUnmount(() => {
  observer?.disconnect()
})
</script>

<template>
  <div ref="el" />
</template>
