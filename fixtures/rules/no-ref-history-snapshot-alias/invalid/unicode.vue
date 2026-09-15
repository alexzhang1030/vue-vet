<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const 源 = ref({ 计数: 1 })
const { commit, undo } = useManualRefHistory(源)
commit()
源.value.计数 = 2
undo()
void 源.value.计数
</script>

<template>
  <p />
</template>
