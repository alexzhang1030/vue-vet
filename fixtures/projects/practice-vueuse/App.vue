<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  setTimeout(() => {
    console.log('ready')
  }, 1000)
})
</script>

<template>
  <div />
</template>
