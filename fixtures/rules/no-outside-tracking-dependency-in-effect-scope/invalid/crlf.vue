<script setup lang="ts">
import { effectScope, ref } from 'vue'
const count = ref(0)
const scope = effectScope()
scope.run(() => {
  Promise.resolve().then(() => {
    void count.value
  })
})
</script>
<template>{{ count }}</template>
