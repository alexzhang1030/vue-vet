<script setup lang="ts">
import { reactive } from 'vue'
const items = reactive([1, 2])
const { includes } = items
includes(1)
</script>

<template>
  <p />
</template>
