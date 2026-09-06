<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const { a } = toRefs(reactive({ a: 1 }))
void a
</script>

<template>
  <p />
</template>
