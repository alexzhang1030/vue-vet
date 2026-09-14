<script setup lang="ts">
import { computed, ref } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
const first = computed(() => doubled.value[0])
void first.value
items.value = [1, 2]
void first.value
</script>

<template>
  <p />
</template>
