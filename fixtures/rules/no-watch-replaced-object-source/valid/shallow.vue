<script setup lang="ts">
import { shallowReactive, watch } from 'vue'
const state = shallowReactive({ p: { x: 1 } })
watch(state.p, () => {})
state.p = { x: 2 }
</script>

<template>
  <p />
</template>
