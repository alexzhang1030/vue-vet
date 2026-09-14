<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ p: { x: 1 } })
watch(state.p, () => {})
function Map() { return state.p }
state.p = new Map()
</script>

<template>
  <p />
</template>
