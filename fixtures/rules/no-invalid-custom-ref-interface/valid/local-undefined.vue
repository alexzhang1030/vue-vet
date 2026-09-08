<script setup lang="ts">
import { customRef } from 'vue'
function make(undefined: () => number) {
  const count = customRef(() => ({ get: undefined }))
  void count.value
}
make(() => 7)
</script>

<template>
  <p />
</template>
