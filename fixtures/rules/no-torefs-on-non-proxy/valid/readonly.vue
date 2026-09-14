<script setup lang="ts">
import { readonly, toRefs } from 'vue'
const { a } = toRefs(readonly({ a: 1 }))
void a
</script>

<template>
  <p />
</template>
