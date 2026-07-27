<script setup lang="ts">
import { reactive } from 'vue'
import Child from './Child.vue'

const bag = reactive({ nested: { name: 'deep' } })
</script>

<template>
  <!-- static multi-hop / optional chain join to root binding `bag` -->
  <Child :subtitle="bag.nested.name" :title="bag?.nested?.name" />
</template>
