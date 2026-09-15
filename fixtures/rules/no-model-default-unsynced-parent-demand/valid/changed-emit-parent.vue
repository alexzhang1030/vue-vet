<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './changed-emit-child.vue'
const value = ref()
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-model="value" />
</template>
