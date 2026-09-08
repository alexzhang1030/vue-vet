<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
scope.stop()
const result = scope.run(() => ({ count: 1 }))
void result?.count
</script>

<template>
  <p />
</template>
