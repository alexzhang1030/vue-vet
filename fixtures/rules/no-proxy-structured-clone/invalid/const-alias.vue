<script setup lang="ts">
import { reactive } from 'vue'
const state = reactive({ count: 1 })
const alias = state
structuredClone(alias)
</script>

<template>
  <p />
</template>
