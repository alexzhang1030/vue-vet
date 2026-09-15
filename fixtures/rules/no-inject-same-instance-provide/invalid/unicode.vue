<script setup lang="ts">
import { inject, provide } from 'vue'
const 键 = Symbol('count')
provide(键, 7)
const 计数 = inject(键, 'missing')
计数.toFixed(2)
</script>

<template>
  <p />
</template>
