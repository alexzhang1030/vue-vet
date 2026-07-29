<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => String(count.value))
</script>

<template>
  <p>{{ label }}</p>
</template>
