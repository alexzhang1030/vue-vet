<script setup lang="ts">
import { customRef } from 'vue'
const 计数 = customRef(() => ({
  set() {},
}))
void 计数.value
</script>

<template>
  <p />
</template>
