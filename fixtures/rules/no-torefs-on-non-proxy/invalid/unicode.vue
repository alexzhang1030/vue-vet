<script setup lang="ts">
import { toRefs } from 'vue'
const 源 = { a: 1 }
const { a } = toRefs(源)
void a
</script>

<template>
  <p />
</template>
