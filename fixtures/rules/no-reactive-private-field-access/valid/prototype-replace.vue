<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
Counter.prototype.read = function read() {
  return 0
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
