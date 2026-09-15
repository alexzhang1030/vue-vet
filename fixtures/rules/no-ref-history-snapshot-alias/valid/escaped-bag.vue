<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const bag = useManualRefHistory(source)
hold(bag)
bag.commit()
source.value.n = 2
bag.undo()
function hold(_value: unknown) {}
</script>

<template>
  <p />
</template>
