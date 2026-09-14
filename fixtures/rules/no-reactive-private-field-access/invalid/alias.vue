<script setup lang="ts">
import { reactive as box } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = box(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
