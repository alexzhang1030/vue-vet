<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2, 3])
const kept = computed(() => items.value.filter((n) => n > 0))
watch(kept, (value) => {
  void value
})
items.value = [1, 2, 3]
</script>

<template>
  <p />
</template>
