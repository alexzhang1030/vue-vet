<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { undo } = useManualRefHistory(source)
source.value.n = 2
undo()
</script>

<template>
  <p />
</template>
