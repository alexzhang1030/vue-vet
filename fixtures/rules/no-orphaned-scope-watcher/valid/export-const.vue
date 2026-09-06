<script setup lang="ts">
import { effectScope, watchEffect } from 'vue'
export const scope = effectScope()
scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {})
})
</script>
<template>ok</template>
