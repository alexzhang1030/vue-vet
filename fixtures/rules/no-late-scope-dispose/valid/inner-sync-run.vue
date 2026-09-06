<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  scope.run(() => {
    onScopeDispose(() => {})
  })
})
</script>
<template>ok</template>
