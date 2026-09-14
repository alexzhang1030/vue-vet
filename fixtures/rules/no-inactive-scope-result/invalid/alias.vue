<script setup lang="ts">
import { effectScope as makeScope } from 'vue'
const scope = makeScope()
scope.stop()
const result = scope.run(() => ({ count: 1 }))
void result.count
</script>

<template>
  <p />
</template>
