<script setup lang="ts">
import { reactive, toRef } from 'vue'
const state = reactive({ n: 1 })
const n = toRef(state, 'n')
void n
</script>

<template>
  <p />
</template>
