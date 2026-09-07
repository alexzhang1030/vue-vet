<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
scope.stop()
false && scope.run(() => ({ count: 1 })).count
</script>

<template>
  <p />
</template>
