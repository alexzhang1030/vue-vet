<script setup lang="ts">
import { ref, toRef } from 'vue'
function opaque(_value: unknown) {}
const count = ref(0)
opaque(count)
const aliased = toRef(count, 'n')
void aliased
</script>

<template>
  <p />
</template>
