<script setup>
import { reactive, ref, watchEffect } from 'vue'
const source = ref(1)
const target = reactive({ value: 0, label: 'Item' })
watchEffect(() => {
  target.value = source.value
})
</script>
<template>{{ target.label }} {{ target.value }}</template>
