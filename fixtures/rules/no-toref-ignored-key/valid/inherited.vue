<script setup lang="ts">
import { toRef } from 'vue'
const proto = { n: 1 }
const source = Object.create(proto) as { n: number }
const n = toRef(source, 'n')
void n
</script>

<template>
  <p />
</template>
