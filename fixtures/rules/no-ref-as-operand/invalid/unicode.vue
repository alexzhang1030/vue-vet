<script setup lang="ts">
import { ref } from 'vue'
const 计数 = ref(0)
const ok = 计数 > 0
</script>
<template>{{ ok }}</template>
