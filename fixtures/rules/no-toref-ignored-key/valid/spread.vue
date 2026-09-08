<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const args = [count, 'n'] as const
const aliased = toRef(...args)
void aliased
</script>

<template>
  <p />
</template>
