<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const b = ref(0)
const c = computed(() => { b.value = a.value; return a.value })
</script>
<template>{{ c }}</template>
