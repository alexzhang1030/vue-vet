<script setup lang="ts">
import { customRef } from 'vue'
const field = customRef(() => ({
  get() {
    const marker = { [1]: 1 }
    return marker
  },
}))
void field.value
field.value = 7
</script>

<template>
  <p />
</template>
