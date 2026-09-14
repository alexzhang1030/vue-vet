<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
void reactive(new Counter()).read()
</script>

<template>
  <p />
</template>
