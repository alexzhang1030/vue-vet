<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref('initial')
const value = computedWithControl(revision, () => source.value)
void value.value
source.value = 1
void value.value
source.value = 'latest'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
