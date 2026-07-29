<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'

const input = useTemplateRef('input')
onMounted(() => {
  console.log(input.value)
})
</script>

<template>
  <input ref="input" aria-label="Input">
</template>
