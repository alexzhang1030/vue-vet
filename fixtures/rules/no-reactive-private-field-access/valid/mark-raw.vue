<script setup lang="ts">
import { markRaw, reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(markRaw(new Counter()))
void proxy.read()
</script>

<template>
  <p />
</template>
