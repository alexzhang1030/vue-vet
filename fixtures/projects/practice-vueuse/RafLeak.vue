<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  const loop = () => {
    requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)
})
</script>

<template>
  <div />
</template>
