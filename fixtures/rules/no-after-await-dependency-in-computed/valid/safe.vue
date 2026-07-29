<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(() => a.value)
</script>
<template>{{ c }}</template>
