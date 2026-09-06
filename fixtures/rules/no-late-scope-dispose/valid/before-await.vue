<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'
const scope = effectScope()
await scope.run(async () => {
  onScopeDispose(() => {})
  await Promise.resolve()
})
</script>
<template>ok</template>
