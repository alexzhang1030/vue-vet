<script setup lang="ts">
import { effectScope, onScopeDispose, ref } from 'vue'
const count = ref(0)
const scope = effectScope()
scope.run(() => {
  onScopeDispose(() => {
    console.log('disposed')
  })
})
void count
</script>
<template>{{ count }}</template>
