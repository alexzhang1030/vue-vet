<script setup lang="ts">
import { ref, toRef as bind } from 'vue'
const count = ref(0)
const aliased = bind(count, 'n')
void aliased
</script>

<template>
  <p />
</template>
