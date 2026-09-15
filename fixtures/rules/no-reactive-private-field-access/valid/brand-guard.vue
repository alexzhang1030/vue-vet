<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  guarded() {
    return #n in this ? this.#n : 0
  }
}
const proxy = reactive(new Counter())
void proxy.guarded()
</script>

<template>
  <p />
</template>
