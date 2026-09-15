<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
proxy.other = 1
void proxy.read()
</script>

<template>
  <p />
</template>
