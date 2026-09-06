<script setup lang="ts">
import { watch } from 'vue'
declare const state: { nested: { x: number } }
watch(state.nested, () => {})
state.nested = { x: 9 }
</script>

<template>
  <p />
</template>
