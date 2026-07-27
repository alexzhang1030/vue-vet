<script setup lang="ts">
import { reactive } from 'vue'
import Child from './Child.vue'

const bag = reactive({ nested: { name: 'deep' } })
</script>

<template>
  <!-- static multi-hop joins to root binding `bag` (not nested key invention) -->
  <Child :subtitle="bag.nested.name" />
</template>
