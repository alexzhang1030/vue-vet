<script setup lang="ts">
import { ref, toRef } from 'vue'
let count = ref(0)
count = { value: 1 } as typeof count
const aliased = toRef(count, 'n')
void aliased
</script>

<template>
  <p />
</template>
