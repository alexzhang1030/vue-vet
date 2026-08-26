<script setup lang="ts">
import { computed, ref, toValue } from 'vue'
const a = ref(0)
const t = ref(0)
const c = computed(() => toValue(() => { t.value = 1; return a.value }))
</script>
<template>{{ c }}</template>
