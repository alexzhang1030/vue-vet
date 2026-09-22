<script setup lang="ts">
import { toRef } from 'vue'
import Child from './Child.vue'
const props = defineProps<{ title: string }>()
const title = toRef(props, 'title')
</script>
<template>
  <Child :title="title" />
</template>
