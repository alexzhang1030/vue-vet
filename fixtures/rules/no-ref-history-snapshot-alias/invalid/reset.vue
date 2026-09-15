<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { reset } = useManualRefHistory(source)
source.value.n = 2
reset()
void source.value.n
</script>

<template>
  <p />
</template>
