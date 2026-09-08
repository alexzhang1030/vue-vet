<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map<string, number>())
const set = map.set
set('a', 1)
</script>

<template>
  <p />
</template>
