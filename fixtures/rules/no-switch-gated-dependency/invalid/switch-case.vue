<script setup lang="ts">
import { computed, ref } from 'vue'

const mode = ref('a')
const count = ref(0)
const label = computed(() => {
  switch (mode.value) {
    case 'a':
      return String(count.value)
    default:
      return 'other'
  }
})
</script>

<template>
  <p>{{ label }}</p>
</template>
