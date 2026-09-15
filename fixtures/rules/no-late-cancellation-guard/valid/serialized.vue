<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
let pending: Promise<void> = Promise.resolve()
watch(source, (value) => {
  pending = pending.then(async () => {
    result.value = await Promise.resolve(value)
  })
})
</script>
<template>{{ result }}</template>
