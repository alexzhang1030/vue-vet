<script setup lang="ts">
import { reactive, readonly, watch } from 'vue'
const state = readonly(reactive({ p: { x: 1 } }))
watch(state.p, () => {})
state.p = { x: 2 }
</script>

<template>
  <p />
</template>
