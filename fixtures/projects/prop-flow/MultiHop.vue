<script setup lang="ts">
import { reactive } from 'vue'
import Child from './Child.vue'

const bag = reactive({ nested: { name: 'deep' } })
</script>

<template>
  <!-- multi-hop member access stays under-approx quiet -->
  <Child :subtitle="bag.nested.name" />
</template>
