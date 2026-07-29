<script setup lang="ts">
import { onMounted, ref } from 'vue'

const width = ref(window.innerWidth)
const height = ref(window.innerHeight)

onMounted(() => {
  window.addEventListener('resize', () => {
    width.value = window.innerWidth
    height.value = window.innerHeight
  })
})
</script>
<template>
  <p>{{ width }}x{{ height }}</p>
</template>
