<script setup lang="ts">
import { reactive } from 'vue'
class Replacement {
  #n = 3
  constructor() {
    return { read: () => 4 }
  }
  read() {
    return this.#n
  }
}
const proxy = reactive(new Replacement())
void proxy.read()
</script>

<template>
  <p />
</template>
