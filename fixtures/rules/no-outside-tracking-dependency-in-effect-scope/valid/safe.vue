<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const count = ref(0)
const scope = effectScope()
scope.run(() => {
  watchEffect(() => {
    void count.value
  })
})
</script>
<template>{{ count }}</template>
