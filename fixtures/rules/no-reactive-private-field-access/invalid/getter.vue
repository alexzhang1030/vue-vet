<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  get value() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
void proxy.value
</script>

<template>
  <p />
</template>
