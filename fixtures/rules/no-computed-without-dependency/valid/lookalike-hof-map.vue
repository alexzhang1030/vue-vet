<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(1)
const api = { map(callback: () => number) { return count.value + callback() } }
const result = computed(() => api.map(() => 42))
</script>
<template><div>{{ result }}</div></template>
