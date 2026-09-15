<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const trigger = {
  get value() { outer.value++; outer.value++; return 0 },
}
const stop = watch(outer, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
const snapshot = trigger.value
stop()
void snapshot
</script>
<template>{{ outer }}{{ inner }}</template>
