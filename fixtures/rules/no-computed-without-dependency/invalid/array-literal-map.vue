<script setup lang="ts">
import { computed } from 'vue'
const result = computed(() => [1].map(() => 42)[0])
</script>
<template><div>{{ result }}</div></template>
