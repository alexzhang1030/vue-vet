<script setup lang="ts">
const useMemoize = (resolver: () => unknown) => resolver
const source = { value: 1 }
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
