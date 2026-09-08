<script setup lang="ts">
import { reactive } from 'vue'
const items = reactive([1, 2])
const { map } = items
map((n) => n)
</script>

<template>
  <p />
</template>
