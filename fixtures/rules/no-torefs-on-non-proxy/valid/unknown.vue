<script setup lang="ts">
import { toRefs } from 'vue'
declare const state: { a: number }
const { a } = toRefs(state)
void a
</script>

<template>
  <p />
</template>
