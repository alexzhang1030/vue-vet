<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'

let frame = 0
onMounted(() => {
  const loop = () => {
    frame = requestAnimationFrame(loop)
  }
  frame = requestAnimationFrame(loop)
})
onBeforeUnmount(() => {
  cancelAnimationFrame(frame)
})
</script>

<template>
  <div />
</template>
