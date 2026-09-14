<script setup lang="ts">
import { ref, watch as observe } from 'vue'
const count = ref(0)
observe(count, { handler() { void count.value }, immediate: true })
</script>

<template>
  <p />
</template>
