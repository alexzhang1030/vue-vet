<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(() => a.value + 1)
</script>
<template>{{ c }}</template>
