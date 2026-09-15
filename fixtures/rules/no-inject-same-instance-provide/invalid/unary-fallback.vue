<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('label')
provide(key, 'text')
const label = inject(key, -1)
label.toUpperCase()
</script>

<template>
  <p />
</template>
