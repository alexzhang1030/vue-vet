<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  onScopeDispose(() => {}, true)
})
</script>
<template>ok</template>
