<script setup lang="ts">
import { effectScope, getCurrentScope, ref, watchEffect } from 'vue'
const owner = effectScope()
const inner = ref(0)
await owner.run(async () => {
  await Promise.resolve()
  const current = getCurrentScope()
  watchEffect(() => {
    void inner.value
  }, { flush: 'sync' })
  void current
})
owner.stop()
</script>
<template>{{ inner }}</template>
