<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref(1)
const { commit, undo } = useManualRefHistory(source)
commit()
source.value = 2
undo()
</script>

<template>
  <p />
</template>
