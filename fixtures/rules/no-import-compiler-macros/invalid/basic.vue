<script setup lang="ts">
import { defineProps } from 'vue'
const props = defineProps<{ title: string }>()
void props
</script>
<template>{{ props.title }}</template>
