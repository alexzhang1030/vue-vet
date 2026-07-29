<script setup lang="ts">
import { computed, ref } from 'vue'

const mode = ref('a')
const count = ref(0)
const label = computed(() => {
  const n = count.value
  switch (mode.value) {
    case 'a':
      return String(n)
    default:
      return 'other'
  }
})
</script>

<template>
  <p>{{ label }}</p>
</template>
