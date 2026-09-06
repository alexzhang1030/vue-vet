<script setup>
import { ref, unref } from 'vue'
const input = ref(2)
const formatted = unref(input).toFixed(2)
</script>
<template>{{ formatted }}</template>
