<script setup lang="ts">
import { shallowReadonly } from 'vue'
structuredClone(shallowReadonly({ count: 1 }))
</script>

<template>
  <p />
</template>
