<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
const result = scope.run(() => ({ count: 1 }))
void result.count
scope.stop()
</script>

<template>
  <p />
</template>
