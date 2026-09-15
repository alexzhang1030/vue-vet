<script setup lang="ts">
import { defineProps } from 'vue'
const props = defineProps<{ title: string }>()
void props
</script>
<template>
  <p>中文前缀</p>{{ props.title }}</template>
