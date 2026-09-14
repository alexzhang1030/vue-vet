<script setup lang="ts">
import * as Vue from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = Vue.reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
