<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
watch(doubled, (value) => {
  void value
})
items.value = [1, 2]
</script>

<template>
  <p />
</template>
