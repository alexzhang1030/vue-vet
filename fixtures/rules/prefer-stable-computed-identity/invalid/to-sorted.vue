<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([2, 1])
const ordered = computed(() => items.value.toSorted((a, b) => a - b))
watch(ordered, (value) => {
  void value
})
items.value = [2, 1]
</script>

<template>
  <p />
</template>
