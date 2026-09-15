<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const value = ref()
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-model="value" />
</template>
