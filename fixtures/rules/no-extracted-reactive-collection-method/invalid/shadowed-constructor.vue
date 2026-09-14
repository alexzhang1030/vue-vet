<script setup lang="ts">
import { reactive } from 'vue'
function configure(ctor: unknown) {
  void ctor
}
const Array = class {}
const capability = Array
configure(capability)
const items = reactive([1])
const { map } = items
map()
</script>

<template>
  <p />
</template>
