<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
const { get } = map
void get
</script>

<template>
  <p />
</template>
