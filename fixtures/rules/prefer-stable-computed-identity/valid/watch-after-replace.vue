<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
items.value = [1, 2]
watch(doubled, (value) => {
  void value
})
</script>

<template>
  <p />
</template>
