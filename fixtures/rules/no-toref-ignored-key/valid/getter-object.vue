<script setup lang="ts">
import { toRef } from 'vue'
const source = {
  get n() {
    return 1
  },
}
const n = toRef(source, 'n')
void n
</script>

<template>
  <p />
</template>
