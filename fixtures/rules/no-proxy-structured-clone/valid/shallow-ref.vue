<script setup lang="ts">
import { shallowRef } from 'vue'
structuredClone(shallowRef({ count: 1 }).value)
</script>

<template>
  <p />
</template>
