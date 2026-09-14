<script setup lang="ts">
const customRef = (factory: () => object) => factory()
const count = customRef(() => ({
  set() {},
})) as { value?: number }
void count.value
</script>

<template>
  <p />
</template>
