<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
function watch(_source: unknown, _handler: unknown) {}
watch(count, { handler() { void count.value } })
</script>

<template>
  <p />
</template>
