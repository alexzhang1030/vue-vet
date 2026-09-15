<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { commit, undo } = useManualRefHistory(source, { clone: true })
commit()
source.value.n = 2
undo()
</script>

<template>
  <p />
</template>
