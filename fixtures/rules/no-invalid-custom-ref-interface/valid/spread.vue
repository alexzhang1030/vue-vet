<script setup lang="ts">
import { customRef } from 'vue'
const extra = { get() { return 1 } }
const count = customRef(() => ({
  ...extra,
  set() {},
}))
void count.value
</script>

<template>
  <p />
</template>
