<script setup lang="ts">
import { effectScope } from 'vue'
function onScopeDispose(_fn: () => void, _failSilently?: boolean) {}
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  onScopeDispose(() => {})
})
</script>
<template>ok</template>
