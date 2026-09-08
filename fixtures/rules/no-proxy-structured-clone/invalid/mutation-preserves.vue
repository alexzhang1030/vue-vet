<script setup lang="ts">
import { reactive } from 'vue'
const state = reactive({ count: 1 })
state.count = 2
structuredClone(state)
</script>

<template>
  <p />
</template>
