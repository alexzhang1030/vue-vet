<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ p: { x: 1 } })
watch(state.p, () => {})
state.p = state.p
</script>

<template>
  <p />
</template>
