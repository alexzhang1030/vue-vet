<script setup lang="ts">
import { computed } from 'vue'
const label = computed(() => 'static')
</script>

<template>
  <p>{{ label }}</p>
</template>
