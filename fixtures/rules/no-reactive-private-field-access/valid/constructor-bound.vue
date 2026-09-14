<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  constructor() {
    this.read = this.read.bind(this)
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
