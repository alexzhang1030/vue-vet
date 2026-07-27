<script setup lang="ts">
import { ref, unref } from '#imports'

const count = ref(0)
const n = unref(count)
void n
</script>

<template>
  <div />
</template>
