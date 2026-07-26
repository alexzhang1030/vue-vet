<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'

function onResize() {
  console.log('resized')
}

onMounted(() => {
  window.addEventListener('resize', onResize)
})
onUnmounted(() => {
  window.removeEventListener('resize', onResize)
})
</script>

<template>
  <div />
</template>
