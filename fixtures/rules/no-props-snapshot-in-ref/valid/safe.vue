<script setup lang="ts">
import { toRef } from 'vue'
const props = defineProps<{ title: string }>()
const title = toRef(props, 'title')
</script>
<template>{{ title }}</template>
