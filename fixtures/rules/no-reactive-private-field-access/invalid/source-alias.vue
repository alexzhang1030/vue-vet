<script setup lang="ts">
import { reactive } from 'vue'
const wrap = reactive
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = wrap(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
