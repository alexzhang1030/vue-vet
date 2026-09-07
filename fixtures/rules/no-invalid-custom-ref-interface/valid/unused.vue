<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  set() {},
}))
void count
</script>

<template>
  <p />
</template>
