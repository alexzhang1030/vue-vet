<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
scope.stop()
scope.run(() => ({ count: 1 }))
</script>

<template>
  <p />
</template>
