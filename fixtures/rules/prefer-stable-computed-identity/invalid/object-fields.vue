<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const source = ref({ n: 1, m: 2 })
const copy = computed(() => ({ n: source.value.n, m: source.value.m }))
watch(copy, (value) => {
  void value
})
source.value = { n: 1, m: 2 }
</script>

<template>
  <p />
</template>
