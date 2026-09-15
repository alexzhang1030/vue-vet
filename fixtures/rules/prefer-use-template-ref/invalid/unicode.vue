<script setup lang="ts">
import { ref } from 'vue'

const input = ref(null)
</script>

<template>
  <p>中文前缀</p>
  <input ref="input" aria-label="Input">
</template>
