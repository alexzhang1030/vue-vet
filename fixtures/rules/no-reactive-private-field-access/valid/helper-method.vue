<script setup lang="ts">
import { reactive } from 'vue'
function helper() {}
class Counter {
  #n = 1
  read() {
    helper()
    return this.#n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
