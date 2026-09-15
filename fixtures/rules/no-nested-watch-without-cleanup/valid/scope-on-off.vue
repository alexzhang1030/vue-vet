<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const owner = effectScope()
const outer = ref(0)
const inner = ref(0)
owner.run(() => {
  watch(outer, () => {
    owner.on()
    watch(inner, () => {}, { flush: 'sync' })
    owner.off()
  }, { flush: 'sync' })
})
</script>
<template>{{ outer }}{{ inner }}</template>
