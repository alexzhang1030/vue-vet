<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { history } = useManualRefHistory(source)
source.value.n = 2
void history.value[0].snapshot.n
</script>

<template>
  <p />
</template>
