<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'

const el = ref<HTMLElement | null>(null)
let observer: MutationObserver | null = null

onMounted(() => {
  observer = new MutationObserver(() => {
    // mutated
  })
  if (el.value) {
    observer.observe(el.value, { childList: true })
  }
})

onBeforeUnmount(() => {
  observer?.disconnect()
})
</script>

<template>
  <div ref="el" />
</template>
