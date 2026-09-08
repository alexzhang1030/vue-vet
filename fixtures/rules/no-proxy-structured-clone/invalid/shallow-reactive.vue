<script setup lang="ts">
import { shallowReactive } from 'vue'
const state = shallowReactive({ count: 1 })
structuredClone(state)
</script>

<template>
  <p />
</template>
