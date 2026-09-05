<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
const alias = count
watchEffect(() => {
  alias.value = count.value + 1
})
</script>
<template>{{ count }}</template>
