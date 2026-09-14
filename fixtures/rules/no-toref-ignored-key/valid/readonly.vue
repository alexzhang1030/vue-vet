<script setup lang="ts">
import { readonly, toRef } from 'vue'
const state = readonly({ n: 1 })
const n = toRef(state, 'n')
void n
</script>

<template>
  <p />
</template>
