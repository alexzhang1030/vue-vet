<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  const el = document.querySelector('#box')
  const observer = new ResizeObserver(() => {
    // size
  })
  if (el) {
    observer.observe(el)
  }
})
</script>

<template>
  <div id="box" />
</template>
