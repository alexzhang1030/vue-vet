<script setup lang="ts">
import { ref } from 'vue'
import { useMutationObserver } from '@vueuse/core'

const el = ref<HTMLElement | null>(null)

useMutationObserver(
  el,
  () => {
    // mutated
  },
  { childList: true },
)
</script>

<template>
  <div ref="el" />
</template>
