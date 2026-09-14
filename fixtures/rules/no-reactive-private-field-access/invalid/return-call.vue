<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
function setup() {
  const proxy = reactive(new Counter())
  return proxy.read()
}
setup()
</script>

<template>
  <p />
</template>
