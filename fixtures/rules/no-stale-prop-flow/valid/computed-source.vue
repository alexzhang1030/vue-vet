<script setup lang="ts">
import { computed } from 'vue'
import Child from './Child.vue'
const title = computed(() => 'hi')
</script>
<template>
  <Child :title="title" />
</template>
