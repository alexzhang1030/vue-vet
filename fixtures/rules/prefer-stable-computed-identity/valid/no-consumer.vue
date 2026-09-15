<script setup lang="ts">
import { computed, ref } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
items.value = [1, 2]
void doubled.value
</script>

<template>
  <p />
</template>
