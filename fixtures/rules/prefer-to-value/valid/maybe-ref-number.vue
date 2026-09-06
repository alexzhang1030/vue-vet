<script setup lang="ts">
import { unref, type MaybeRef } from 'vue'
function format(input: MaybeRef<number>) {
  return unref(input).toFixed(2)
}
</script>
<template>{{ format(2) }}</template>
