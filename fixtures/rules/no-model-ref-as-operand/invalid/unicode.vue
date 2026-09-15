<script setup lang="ts">
const file = defineModel<File | null>('file', { default: null })
const ok = !file
</script>

<template>
  <p>中文前缀</p>
  <p>{{ ok }}</p>
</template>
