<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  get: 1,
  set() {},
}))
void count.value
</script>

<template>
  <p />
</template>
