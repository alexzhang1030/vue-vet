<script setup lang="ts">
import { customRef, ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const trigger = ref(customRef(() => ({
  get() { outer.value++; outer.value++; return 0 },
  set() {},
})))
const stop = watch(outer, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
const snapshot = trigger.value
stop()
void snapshot
</script>
<template>{{ outer }}{{ inner }}</template>
