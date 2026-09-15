<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  onScopeDispose(() => {})
})
</script>
<template>
  <p>中文前缀</p>ok</template>
