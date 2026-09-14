<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
const key = 'equals'
watch(count, (n) => void n, { [key]: () => true })
</script>

<template>
  <p />
</template>
