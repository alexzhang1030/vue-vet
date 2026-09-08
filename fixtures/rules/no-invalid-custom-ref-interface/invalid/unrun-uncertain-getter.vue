<script setup lang="ts">
import { customRef } from 'vue'
const field = customRef(() => ({
  get() {
    const marker = { [this._set = (value: number) => { void value }]: 1 }
    return 1
  },
}))
field.value = 7
</script>

<template>
  <p />
</template>
