<script setup lang="ts">
import { ref } from 'vue'

const state = ref(null)
console.log(state.value)
</script>

<template>
  <input ref="input">
</template>
