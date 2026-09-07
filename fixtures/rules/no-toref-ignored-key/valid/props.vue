<script setup lang="ts">
import { toRef } from 'vue'
const props = defineProps<{ n: number }>()
const n = toRef(props, 'n')
void n
</script>

<template>
  <p />
</template>
