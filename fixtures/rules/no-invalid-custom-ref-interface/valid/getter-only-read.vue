<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  get() {
    return 1
  },
}))
void count.value
</script>

<template>
  <p />
</template>
