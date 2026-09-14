<script setup lang="ts">
import { reactive } from 'vue'
const items = reactive([1, 2])
const push = items.push
push(3)
</script>

<template>
  <p />
</template>
