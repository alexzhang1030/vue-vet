<script setup lang="ts">
import * as Vue from 'vue'
const scope = Vue.effectScope()
scope.stop()
const result = scope.run(() => ({ count: 1 }))
void result.count
</script>

<template>
  <p />
</template>
