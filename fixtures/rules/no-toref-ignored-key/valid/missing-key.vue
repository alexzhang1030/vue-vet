<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const boxed = toRef(count)
void boxed
</script>

<template>
  <p />
</template>
