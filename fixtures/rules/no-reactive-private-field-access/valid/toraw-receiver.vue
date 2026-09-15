<script setup lang="ts">
import { reactive, toRaw } from 'vue'
class Counter {
  #n = 1
  read() {
    return toRaw(this).#n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>

<template>
  <p />
</template>
