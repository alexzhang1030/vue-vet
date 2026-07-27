<script setup lang="ts">
defineProps<{
  title?: string
  name?: string
}>()
</script>

<template>
  <p>{{ title }} · {{ name }}</p>
</template>
