<script setup lang="ts">
import { computed, type Ref } from 'vue'
function consume(value: Ref<boolean>) {
  return value.value
}
const enabled = consume(computed(() => true))
</script>
<template>{{ enabled }}</template>
