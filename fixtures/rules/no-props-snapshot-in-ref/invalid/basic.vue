<script setup lang="ts">
import { ref } from 'vue'
const props = defineProps<{ title: string }>()
const snapshot = ref(props)
void snapshot
</script>
<template>{{ props.title }}</template>
