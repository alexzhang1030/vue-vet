<script setup lang="ts">
import { ref, watch as observe } from 'vue'
const n = ref(0)
observe(n.value, () => {})
</script>

<template>
  <p />
</template>
