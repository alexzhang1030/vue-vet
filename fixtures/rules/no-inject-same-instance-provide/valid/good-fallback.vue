<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 0)
count.toFixed(2)
</script>

<template>
  <p />
</template>
