<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'

let timer: ReturnType<typeof setTimeout> | undefined
onMounted(() => {
  timer = setTimeout(() => {
    console.log('ready')
  }, 1000)
})
onBeforeUnmount(() => {
  if (timer !== undefined) {
    window.clearTimeout(timer)
  }
})
</script>

<template>
  <div />
</template>
