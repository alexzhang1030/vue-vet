<script setup lang="ts">
import { computed, shallowRef } from 'vue'
const state = shallowRef({ count: 1 })
const doubled = computed(() => state.value.count * 2)
state.value.count = 2
</script>
<template>{{ doubled }}</template>
