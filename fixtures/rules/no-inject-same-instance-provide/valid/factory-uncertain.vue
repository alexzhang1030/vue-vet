<script setup lang="ts">
import { inject, provide } from 'vue'
const flag = true
const key = Symbol('count')
provide(key, 7)
const count = inject(key, () => 'missing', flag)
count.toFixed(2)
</script>

<template>
  <p />
</template>
