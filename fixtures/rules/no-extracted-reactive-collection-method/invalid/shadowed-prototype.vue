<script setup lang="ts">
import { reactive } from 'vue'
function configure(prototype: unknown) {
  void prototype
}
const Array = { prototype: {} }
const capability = Array.prototype
configure(capability)
const items = reactive([1])
const { map } = items
map()
</script>

<template>
  <p />
</template>
