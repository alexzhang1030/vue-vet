<script setup lang="ts">
import { toRefs } from 'vue'
const obj = { a: 1 }
const { a } = toRefs(obj)
void a
</script>

<template>
  <p />
</template>
