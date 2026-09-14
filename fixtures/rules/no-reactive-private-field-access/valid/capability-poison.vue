<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
proxy.read = () => 0
void proxy.read()
</script>

<template>
  <p />
</template>
