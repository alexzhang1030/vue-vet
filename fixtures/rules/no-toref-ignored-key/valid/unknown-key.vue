<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const key = 'n'
const aliased = toRef(count, key)
void aliased
</script>

<template>
  <p />
</template>
