<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const bag = useManualRefHistory(source)
bag.commit()
source.value.n = 2
bag.undo()
void source.value.n
</script>

<template>
  <p />
</template>
