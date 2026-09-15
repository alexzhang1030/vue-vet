<script setup lang="ts">
import { inject } from 'vue'
const key = Symbol('count')
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
