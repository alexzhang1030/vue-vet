<script setup lang="ts">
import { shallowReactive, shallowReadonly } from 'vue'
const a = shallowReactive(null)
const b = shallowReadonly(undefined)
void a
void b
</script>

<template>
  <p />
</template>
