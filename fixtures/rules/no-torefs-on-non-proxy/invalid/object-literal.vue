<script setup lang="ts">
import { toRefs } from 'vue'
const { a } = toRefs({ a: 1 })
void a
</script>

<template>
  <p />
</template>
