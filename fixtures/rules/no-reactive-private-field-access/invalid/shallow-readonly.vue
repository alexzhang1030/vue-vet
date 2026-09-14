<script setup lang="ts">
import { shallowReadonly } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = shallowReadonly(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
