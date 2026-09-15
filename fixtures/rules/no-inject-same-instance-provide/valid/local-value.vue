<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
const local = 7
provide(key, local)
local.toFixed(2)
const count = inject(key, 0)
count.toFixed(2)
</script>

<template>
  <p />
</template>
