<script setup lang="ts">
import { computed } from 'vue'
declare function createState(): { current: Set<string> }
const state = createState()
const active = computed(() => state.current.size === 1)
</script>
<template>{{ active }}</template>
