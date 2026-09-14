<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
function retain(ctor: typeof Counter) {
  void ctor
}
retain(Counter)
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
