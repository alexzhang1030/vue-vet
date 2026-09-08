<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const aliased = toRef(count as object, 'n')
void aliased
</script>

<template>
  <p />
</template>
