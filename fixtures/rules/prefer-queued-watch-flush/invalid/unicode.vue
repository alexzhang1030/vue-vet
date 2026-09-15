<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'

const 源 = ref(0)
const 汇 = ref(0)
watch(源, (value) => { 汇.value = value }, { flush: 'sync' })
源.value = 1
源.value = 2
await nextTick()
void 汇.value
</script>

<template>
  <p />
</template>
