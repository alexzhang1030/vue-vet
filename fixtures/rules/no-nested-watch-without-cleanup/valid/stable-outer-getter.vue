<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const owner = effectScope()
owner.run(() => {
  watch(() => { void outer.value; return 0 }, () => {
    watch(inner, () => {}, { flush: 'sync' })
  }, { immediate: true, flush: 'sync' })
})
</script>
<template>{{ outer }}{{ inner }}</template>
