<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const joined = computed(() => items.value.concat([3]))
watch(joined, (value) => {
  void value
})
items.value = [1, 2]
</script>

<template>
  <p />
</template>
