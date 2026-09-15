<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  static #n = 1
  read() {
    return Counter.#n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
