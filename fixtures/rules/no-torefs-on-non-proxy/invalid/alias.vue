<script setup lang="ts">
import { toRefs as split } from 'vue'
const { a } = split({ a: 1 })
void a
</script>

<template>
  <p />
</template>
