<script setup lang="ts">
import { ref, unref } from 'vue'

const count = ref(0)
const n = unref(count)
void n
</script>

<template>
  <div />
</template>
