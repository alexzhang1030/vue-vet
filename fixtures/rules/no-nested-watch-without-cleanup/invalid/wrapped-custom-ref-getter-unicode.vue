<script setup lang="ts">
import { customRef, ref, watch } from 'vue'
const 外层 = ref(0)
const 内层 = ref(0)
const 触发 = ref(customRef(() => ({
  get() { 外层.value++; 外层.value++; return 0 },
  set() {},
})))
const stop = watch(外层, () => {
  watch(内层, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
const snapshot = 触发.value
stop()
void snapshot
</script>
<template>{{ 外层 }}{{ 内层 }}</template>
