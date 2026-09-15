<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const bag = useManualRefHistory(source)
source.value.n = 2
const result = typeof bag.history
void result
</script>

<template>
  <p />
</template>
