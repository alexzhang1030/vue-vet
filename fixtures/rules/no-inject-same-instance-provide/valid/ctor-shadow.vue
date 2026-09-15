<script setup lang="ts">
import { inject, provide } from 'vue'
const Symbol = () => 'not-a-symbol'
const key = Symbol()
provide(key as never, 7)
const count = inject(key as never, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
