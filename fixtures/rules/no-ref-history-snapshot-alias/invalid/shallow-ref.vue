<script setup lang="ts">
import { shallowRef } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = shallowRef({ n: 1 })
const { commit, undo } = useManualRefHistory(source)
commit()
source.value.n = 2
undo()
void source.value.n
</script>

<template>
  <p />
</template>
