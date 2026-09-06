<script setup lang="ts">
import { reactive, watch as observe } from 'vue'
const state = reactive({ nested: { x: 1 } })
observe(state.nested, () => {})
state.nested = { x: 9 }
</script>

<template>
  <p />
</template>
