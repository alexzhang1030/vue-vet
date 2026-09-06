<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ p: { x: 1 } })
watch(state.p, () => {})
state.p ||= { x: 2 }
</script>

<template>
  <p />
</template>
