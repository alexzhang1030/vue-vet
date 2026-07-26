<script setup lang="ts">
import { useEventListener } from '@vueuse/core'

useEventListener(window, 'resize', () => {
  console.log('resized')
})
</script>

<template>
  <div />
</template>
