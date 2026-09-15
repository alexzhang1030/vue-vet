<script setup lang="ts">
import { effectScope, ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const scope = effectScope()
const revision = ref(0)
const source = ref(1)
const value = computedWithControl(revision, () => source.value)
void value.value
scope.stop()
source.value = 'text'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
