<script setup lang="ts">
import { inject, provide } from 'vue'
// Out of first-slice scope: inner() still runs on this instance and throws.
function inner() {
  const key = Symbol('count')
  provide(key, 7)
  const count = inject(key, 'missing')
  count.toFixed(2)
}
inner()
</script>

<template>
  <p />
</template>
