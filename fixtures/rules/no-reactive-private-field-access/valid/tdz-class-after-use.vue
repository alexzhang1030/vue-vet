<script setup lang="ts">
import { reactive } from 'vue'
const proxy = reactive(new Counter())
void proxy.read()
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
</script>

<template>
  <p />
</template>
