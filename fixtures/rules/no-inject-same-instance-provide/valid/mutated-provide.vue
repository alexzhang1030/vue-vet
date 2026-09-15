<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
provide(key, 'text')
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
