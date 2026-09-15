<script setup lang="ts">
import { inject, provide } from 'vue'
function use(value: string) { return value }
const key = Symbol('count')
provide(key, 7)
const count = use(inject(key, 'missing'))
count.toFixed(2)
</script>

<template>
  <p />
</template>
