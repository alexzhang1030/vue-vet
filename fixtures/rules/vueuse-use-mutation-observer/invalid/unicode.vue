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
  <p>中文前缀</p>
  <div ref="el" />
</template>
