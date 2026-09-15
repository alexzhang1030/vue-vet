<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  private n = 1
  read() {
    return this.n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
