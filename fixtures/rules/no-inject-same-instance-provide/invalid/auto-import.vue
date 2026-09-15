<script setup lang="ts">
import { inject, provide } from '#imports'
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
