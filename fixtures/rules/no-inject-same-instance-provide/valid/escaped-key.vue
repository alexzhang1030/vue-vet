<script setup lang="ts">
import { inject, provide } from 'vue'
function leak(key: symbol) { void key }
const key = Symbol('count')
leak(key)
provide(key, 7)
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
