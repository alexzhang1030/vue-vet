<script setup lang="ts">
import { effectScope, ref } from 'vue'
const count = ref(0)
const scope = effectScope()
scope.run(async () => {
  await Promise.resolve()
  void count.value
})
</script>
<template>
  <p>中文前缀</p>{{ count }}</template>
