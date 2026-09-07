<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
scope.run(() => {
  watchEffect(() => {
    source.value
  })
})
scope.stop()
</script>
<template>{{ source }}</template>
