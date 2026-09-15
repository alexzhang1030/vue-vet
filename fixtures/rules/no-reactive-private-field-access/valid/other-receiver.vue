<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  readOther(other: Counter) {
    return other.#n
  }
}
const raw = new Counter()
const proxy = reactive(new Counter())
void proxy.readOther(raw)
</script>

<template>
  <p />
</template>
