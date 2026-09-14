<script setup lang="ts">
import { customRef } from 'vue'
let saved = 0
const field = customRef(() => ({
  get() {
    const marker = { [this._set = (value: number) => { saved = value }]: 1 }
    return marker
  },
}))
void field.value
field.value = 7
void saved
</script>

<template>
  <p />
</template>
