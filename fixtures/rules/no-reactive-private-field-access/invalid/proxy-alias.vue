<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
const alias = proxy
void alias.read()
</script>

<template>
  <p />
</template>
