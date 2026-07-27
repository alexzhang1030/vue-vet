<script setup lang="ts">
import { onMounted, ref } from 'vue'

const el = ref<HTMLElement | null>(null)

onMounted(() => {
  const observer = new MutationObserver(() => {
    // mutated
  })
  if (el.value) {
    observer.observe(el.value, { childList: true })
  }
})
</script>

<template>
  <div ref="el" />
</template>
