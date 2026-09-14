<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
const handle = scope.run(() => scope)
void handle
scope.stop()
</script>

<template>
  <p />
</template>
