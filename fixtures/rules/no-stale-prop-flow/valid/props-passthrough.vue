<script setup lang="ts">
import Child from './Child.vue'
const props = defineProps<{ title: string }>()
</script>
<template>
  <Child :title="props.title" />
</template>
