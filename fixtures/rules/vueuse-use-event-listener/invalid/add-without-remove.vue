<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  window.addEventListener('resize', () => {
    console.log('resized')
  })
})
</script>

<template>
  <div />
</template>
