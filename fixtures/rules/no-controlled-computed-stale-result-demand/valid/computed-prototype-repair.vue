<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
Number['prototype']['toUpperCase'] = function () {
  return 'supported'
}
const revision = ref(0)
const source = ref(1)
const value = computedWithControl(revision, () => source.value)
void value.value
source.value = 'latest'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
