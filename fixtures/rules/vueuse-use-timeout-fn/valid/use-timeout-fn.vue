<script setup lang="ts">
import { useTimeoutFn } from '@vueuse/core'

useTimeoutFn(() => {
  console.log('ready')
}, 1000)
</script>

<template>
  <div />
</template>
