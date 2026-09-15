<script setup lang="ts">
import { type inject, type provide } from 'vue'
const provide = (_key: symbol, _value: number) => {}
const inject = (_key: symbol, fallback: string) => fallback
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
