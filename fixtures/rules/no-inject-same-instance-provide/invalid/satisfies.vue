<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 'missing')
;(count satisfies unknown as number).toFixed(2)
</script>

<template>
  <p />
</template>
