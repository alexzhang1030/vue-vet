<script setup lang="ts">
import { onMounted } from 'vue'

onMounted(() => {
  const el = document.querySelector('#box')
  const observer = new MutationObserver(() => {
    // mutated
  })
  if (el) {
    observer.observe(el, { childList: true })
  }
})
</script>

<template>
  <div id="box" />
</template>
