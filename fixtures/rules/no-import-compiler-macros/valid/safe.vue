<script setup lang="ts">
const props = defineProps<{ title: string }>()
void props
</script>
<template>{{ props.title }}</template>
