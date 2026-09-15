<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
const ok = doubled > 0
</script>

<template>
  <p>中文前缀</p>
  <p>{{ ok }}</p>
</template>
