<script setup lang="ts">
import { inject as take, provide as give } from 'vue'
const key = Symbol('count')
give(key, 7)
const count = take(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
