<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  Promise.resolve().then(() => count.value)
  return 0
})
</script>
<template>
  <p>{{ label }}</p>
</template>
