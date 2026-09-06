<script setup lang="ts">
const file = defineModel<File | null>('file', { default: null })
const ok = !file
</script>

<template>
  <p>{{ ok }}</p>
</template>
