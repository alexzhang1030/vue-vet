<script setup lang="ts">
import { ref } from 'vue'
import { useRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { history } = useRefHistory(source)
source.value.n = 2
void history.value
</script>

<template>
  <p />
</template>
