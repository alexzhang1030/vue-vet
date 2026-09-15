<script setup lang="ts">
import { computed, ref } from 'vue'
const source = ref(1)
const value = computed(() => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
