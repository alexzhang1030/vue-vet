<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
function load() {
  return String(count.value)
}
const label = computed(() => load())
</script>
<template>
  <p>{{ label }}</p>
</template>
