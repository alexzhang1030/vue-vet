<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1])
items.value = [2]
const doubled = computed(() => items.value.map((n) => n * 2))
void doubled.value
watch(doubled, (value) => {
  void value
}, { flush: 'sync' })
items.value = [1]
</script>

<template>
  <p />
</template>
