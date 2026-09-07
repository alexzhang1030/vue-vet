<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  set() {},
}))
count._get = () => 7
void count.value
</script>

<template>
  <p />
</template>
