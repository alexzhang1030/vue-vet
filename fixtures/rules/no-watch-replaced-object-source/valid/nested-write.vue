<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ nested: { x: 1 } })
watch(state.nested, () => {})
state.nested.x = 2
</script>

<template>
  <p />
</template>
