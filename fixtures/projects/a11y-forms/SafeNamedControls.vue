<template>
  <a href="/docs" aria-label="Documentation">
    <div class="i-carbon-document" />
  </a>
  <button type="button" aria-label="Settings">
    <div class="i-carbon-settings" />
  </button>
  <h2>{{ title }}</h2>
</template>

<script setup lang="ts">
const title = 'Overview'
</script>
