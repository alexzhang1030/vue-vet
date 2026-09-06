<script setup lang="ts">
import { computed, type Ref } from 'vue'
function consume(value: Ref<string>) {
  return value.value
}
const label = import.meta.env.SSR ? consume(computed(() => 'static')) : consume(computed(() => 'client'))
</script>
<template>{{ label }}</template>
