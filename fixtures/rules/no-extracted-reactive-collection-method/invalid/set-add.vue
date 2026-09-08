<script setup lang="ts">
import { reactive } from 'vue'
const items = reactive(new Set<number>())
const { add } = items
add(1)
</script>

<template>
  <p />
</template>
