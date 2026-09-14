<script setup lang="ts">
const effectScope = () => ({
  stop() {},
  run<T>(fn: () => T) {
    return fn()
  },
})
const scope = effectScope()
scope.stop()
const result = scope.run(() => ({ count: 1 }))
void result.count
</script>

<template>
  <p />
</template>
