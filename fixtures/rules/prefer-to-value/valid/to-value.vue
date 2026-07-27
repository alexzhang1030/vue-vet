<script setup lang="ts">
import { ref, toValue } from 'vue'

const count = ref(0)
const n = toValue(count)
void n
</script>

<template>
  <div />
</template>
