<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const missing = toRef(count, undefined)
void missing
</script>

<template>
  <p />
</template>
