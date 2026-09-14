<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  constructor() {
    void new.target
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
