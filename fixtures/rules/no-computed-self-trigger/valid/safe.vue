<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const result = computed(() => count.value + 1)
</script>
<template>{{ result }}</template>
