<script setup lang="ts">
import { useTemplateRef } from 'vue'

const input = useTemplateRef('input')
console.log(input.value)
</script>

<template>
  <input ref="input">
</template>
