<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n
  constructor() {
    this.#n = 5
    this.items = []
  }
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
