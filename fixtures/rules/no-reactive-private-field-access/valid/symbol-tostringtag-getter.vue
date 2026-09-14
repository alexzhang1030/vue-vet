<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  get [Symbol.toStringTag]() {
    return 'Counter'
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
