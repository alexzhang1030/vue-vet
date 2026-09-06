<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
const ok = doubled.value > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>
