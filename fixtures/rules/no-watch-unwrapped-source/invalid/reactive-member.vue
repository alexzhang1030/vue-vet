<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ n: 0 })
watch(state.n, () => {})
</script>

<template>
  <p />
</template>
