<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const count = ref(2)
const doubled = computed(() => count.value * 2)
watch(doubled, (value) => {
  void value
})
count.value = 2
</script>

<template>
  <p />
</template>
