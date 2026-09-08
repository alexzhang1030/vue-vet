<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  get() {
    this._set = (value: number) => {
      this.saved = value
    }
    return 1
  },
}))
void count.value
count.value = 7
</script>

<template>
  <p />
</template>
