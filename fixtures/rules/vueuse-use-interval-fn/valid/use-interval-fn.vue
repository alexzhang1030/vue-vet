<script setup lang="ts">
import { useIntervalFn } from '@vueuse/core'

useIntervalFn(() => {
  console.log('tick')
}, 1000)
</script>

<template>
  <div />
</template>
