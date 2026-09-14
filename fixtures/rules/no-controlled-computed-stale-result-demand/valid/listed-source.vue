<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = computedWithControl([revision, source], () => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
