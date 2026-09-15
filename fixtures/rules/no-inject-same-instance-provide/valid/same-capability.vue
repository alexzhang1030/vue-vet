<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 'text')
const count = inject(key, 'missing')
count.toUpperCase()
</script>

<template>
  <p />
</template>
