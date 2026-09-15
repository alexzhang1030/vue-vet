<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2, 3])
const copy = computed(() => items.value.slice())
watch(copy, (value) => {
  void value
})
items.value = [1, 2, 3]
</script>

<template>
  <p />
</template>
