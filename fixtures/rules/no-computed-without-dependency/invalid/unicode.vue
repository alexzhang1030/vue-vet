<script setup lang="ts">
import { computed } from 'vue'
const result = computed(() => [1].map(() => 42)[0])
</script>
<template>
  <p>中文前缀</p><div>{{ result }}</div></template>
