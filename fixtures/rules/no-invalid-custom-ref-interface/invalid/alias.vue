<script setup lang="ts">
import { customRef as make } from 'vue'
const count = make(() => ({
  set() {},
}))
void count.value
</script>

<template>
  <p />
</template>
