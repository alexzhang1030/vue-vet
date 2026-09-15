<script setup lang="ts">
import { computed, effectScope, ref, watch } from 'vue'
const outer = computed(() => 0)
const inner = ref(0)
const owner = effectScope()
owner.run(() => {
  watch(outer, () => {
    watch(inner, () => {}, { flush: 'sync' })
  }, { immediate: true, flush: 'sync' })
})
</script>
<template>{{ inner }}</template>
