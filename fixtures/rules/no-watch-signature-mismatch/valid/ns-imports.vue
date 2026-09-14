<script setup lang="ts">
import * as Auto from '#imports'
const count = { value: 0 }
Auto.watchEffect(() => count.value, (n: unknown) => void n)
</script>

<template>
  <p />
</template>
