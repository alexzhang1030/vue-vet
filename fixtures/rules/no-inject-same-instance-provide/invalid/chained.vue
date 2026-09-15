<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
inject(key, 'missing').toFixed(2)
</script>

<template>
  <p />
</template>
