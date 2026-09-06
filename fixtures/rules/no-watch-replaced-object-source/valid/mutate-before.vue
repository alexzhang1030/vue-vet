<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ nested: { x: 1 } })
state.nested = { x: 2 }
watch(state.nested, () => {})
</script>

<template>
  <p />
</template>
