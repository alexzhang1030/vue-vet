<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  setInterval(() => {
    console.log('tick')
  }, 1000)
})
</script>

<template>
  <div />
</template>
