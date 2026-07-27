<script setup lang="ts">
import { computed, inject } from 'vue'

const count = inject('count')
const doubled = computed(() => count.value * 2)
</script>

<template>
  <p>{{ doubled }}</p>
</template>
