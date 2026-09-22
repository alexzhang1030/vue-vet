<script setup lang="ts">
import Child from './Child.vue'
const { title } = defineProps<{ title: string }>()
</script>
<template>
  <Child :title="title" />
</template>
