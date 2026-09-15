<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([0])
const copy = computed(() => items.value.map((n) => n))
watch(copy, (value) => {
  void value
})
items.value = [-0]
</script>

<template>
  <p />
</template>
