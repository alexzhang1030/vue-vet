<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const seed = [1, 2]
const items = ref(seed)
const doubled = computed(() => items.value.map((n) => n * 2))
watch(doubled, (value) => {
  void value
})
items.value = seed
</script>

<template>
  <p />
</template>
