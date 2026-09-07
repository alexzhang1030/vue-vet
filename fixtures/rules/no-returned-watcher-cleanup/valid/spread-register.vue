<script setup lang="ts">
import { watchEffect } from 'vue'
watchEffect((onCleanup) => {
  const dispose = () => {}
  onCleanup(...[dispose])
  return dispose
})
</script>
<template>ok</template>
