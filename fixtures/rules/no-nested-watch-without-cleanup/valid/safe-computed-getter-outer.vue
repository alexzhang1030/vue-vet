<script setup lang="ts">
import { computed, effectScope, ref, watch } from 'vue'
const source = computed(() => 0)
const inner = ref(0)
const owner = effectScope()
owner.run(() => {
  watch(() => source.value, () => {
    watch(inner, () => {}, { flush: 'sync' })
  }, { immediate: true, flush: 'sync' })
})
</script>
<template>{{ inner }}</template>
