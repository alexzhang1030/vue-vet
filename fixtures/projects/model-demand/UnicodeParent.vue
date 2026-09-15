<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const 值 = ref()
onMounted(() => {
  值.value.toFixed(2)
})
</script>
<template>
  <Child v-model="值" />
</template>
