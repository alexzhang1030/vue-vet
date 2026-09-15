<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory as historyOf } from '@vueuse/core'
const source = ref({ n: 1 })
const { commit, undo } = historyOf(source)
commit()
source.value.n = 2
undo()
void source.value.n
</script>

<template>
  <p />
</template>
