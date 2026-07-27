<script setup lang="ts">
const props = defineProps<{
  title: string
  modelValue: string
  subtitle: string
  count: string
}>()
</script>

<template>
  <h1>{{ props.title }}</h1>
  <p>{{ props.modelValue }} · {{ props.subtitle }} · {{ props.count }}</p>
</template>
