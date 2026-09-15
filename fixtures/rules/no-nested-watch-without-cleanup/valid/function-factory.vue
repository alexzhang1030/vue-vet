<script setup lang="ts">
import { ref, watch } from 'vue'
const inner = ref(0)
function listen() {
  watch(inner, () => {}, { flush: 'sync' })
}
listen()
</script>
<template>{{ inner }}</template>
