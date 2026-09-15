<script setup lang="ts">
import { ref } from 'vue'
import type { useManualRefHistory } from '@vueuse/core'
const useManualRefHistory = (source: { value: { n: number } }) => ({
  commit() {},
  undo() {},
})
const source = ref({ n: 1 })
const { commit, undo } = useManualRefHistory(source)
commit()
source.value.n = 2
undo()
</script>

<template>
  <p />
</template>
