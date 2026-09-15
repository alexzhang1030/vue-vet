<script setup lang="ts">
import { readonly } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = readonly(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
