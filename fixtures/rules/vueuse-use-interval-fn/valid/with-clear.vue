<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'

let timer: ReturnType<typeof setInterval> | undefined
onMounted(() => {
  timer = setInterval(() => {
    console.log('tick')
  }, 1000)
})
onBeforeUnmount(() => {
  if (timer !== undefined) {
    clearInterval(timer)
  }
})
</script>

<template>
  <div />
</template>
