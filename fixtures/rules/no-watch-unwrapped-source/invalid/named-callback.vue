<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function onChange() {}
watch(n.value, onChange)
</script>

<template>
  <p />
</template>
