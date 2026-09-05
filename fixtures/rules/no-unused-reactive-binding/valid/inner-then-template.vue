<script setup lang="ts">
import { ref } from 'vue'
function make() {
  const count = ref(2)
  return count
}
const count = ref(1)
void make
</script>

<template>
  <p>{{ count }}</p>
</template>
