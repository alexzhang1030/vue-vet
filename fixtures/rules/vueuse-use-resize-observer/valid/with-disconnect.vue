<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'

const el = ref<HTMLElement | null>(null)
let observer: ResizeObserver | null = null

onMounted(() => {
  observer = new ResizeObserver(() => {
    // size
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
