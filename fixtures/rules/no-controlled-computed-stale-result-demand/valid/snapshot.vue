<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = computedWithControl(revision, () => source.value)
const snapshot = value.value
source.value = 'text'
void snapshot
</script>

<template>
  <p />
</template>
