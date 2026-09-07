<script setup lang="ts">
import { customRef } from 'vue'
let out: unknown
out = customRef(() => ({
  get() {
    return 7
  },
})).value
void out
</script>

<template>
  <p />
</template>
