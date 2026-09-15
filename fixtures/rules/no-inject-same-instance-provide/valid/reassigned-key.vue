<script setup lang="ts">
import { inject, provide } from 'vue'
let key = Symbol('count')
key = Symbol('other')
provide(key, 7)
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
