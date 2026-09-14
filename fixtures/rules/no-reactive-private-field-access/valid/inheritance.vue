<script setup lang="ts">
import { reactive } from 'vue'
class Base {
  #n = 1
  read() {
    return this.#n
  }
}
class Child extends Base {}
const proxy = reactive(new Child())
void proxy.read()
</script>

<template>
  <p />
</template>
