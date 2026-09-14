<script setup lang="ts">
import { ref, watch } from 'vue'
const fn = ref(() => 1)
watch(fn.value, () => {})
</script>

<template>
  <p />
</template>
