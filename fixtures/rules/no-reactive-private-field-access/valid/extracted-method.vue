<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
const fn = proxy.read
void fn()
</script>

<template>
  <p />
</template>
