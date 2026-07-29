<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count > 0
</script>
<template>{{ ok }}</template>
