<script setup lang="ts">
import { reactive, watch } from 'vue'
declare const input: { p: { x: number } }
const state = reactive({ p: 1, ...input })
watch(state.p, () => {})
state.p = { x: 2 }
</script>

<template>
  <p />
</template>
