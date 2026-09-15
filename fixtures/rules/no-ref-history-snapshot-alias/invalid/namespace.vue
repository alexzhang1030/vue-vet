<script setup lang="ts">
import { ref } from 'vue'
import * as VueUse from '@vueuse/core'
const source = ref({ n: 1 })
const { commit, undo } = VueUse.useManualRefHistory(source)
commit()
source.value.n = 2
undo()
void source.value.n
</script>

<template>
  <p />
</template>
