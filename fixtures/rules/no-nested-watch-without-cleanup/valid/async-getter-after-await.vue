<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const source = ref(0)
const inner = ref(0)
const owner = effectScope()
owner.run(() => {
  watch(async () => {
    await Promise.resolve()
    return source.value
  }, () => {
    watch(inner, () => {}, { flush: 'sync' })
  }, { immediate: true, flush: 'sync' })
})
</script>
<template>{{ source }}{{ inner }}</template>
