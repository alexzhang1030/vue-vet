<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
const onChange = { handler() { void count.value } }
watch(count, onChange)
</script>

<template>
  <p />
</template>
