<script setup lang="ts">
import { customRef } from 'vue'
function factory() {
  return { set() {} }
}
const count = customRef(factory)
void count.value
</script>

<template>
  <p />
</template>
