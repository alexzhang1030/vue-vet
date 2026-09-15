<script setup lang="ts">
import { ref } from 'vue'
import Child from './Child.vue'
const title = ref('hi')
</script>
<template>
  <Child :title="title" />
</template>
