<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  get() {
    return 1
  },
}))
count.value = 2
</script>

<template>
  <p />
</template>
