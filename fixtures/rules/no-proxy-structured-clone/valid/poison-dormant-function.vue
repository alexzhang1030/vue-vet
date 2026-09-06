<script setup lang="ts">
import { reactive } from 'vue'
function replaceNative() {
  structuredClone = ((value: unknown) => value) as typeof structuredClone
}
void replaceNative
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
