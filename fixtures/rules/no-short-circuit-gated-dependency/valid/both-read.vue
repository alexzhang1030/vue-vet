<script setup lang="ts">
import { computed, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)
const label = computed(() => {
  const n = count.value
  return enabled.value ? String(n) : 'off'
})
</script>

<template>
  <p>{{ label }}</p>
</template>
