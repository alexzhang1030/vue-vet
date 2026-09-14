<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
watch((n.value) as number, () => {})
</script>

<template>
  <p />
</template>
