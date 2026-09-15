<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const value = ref()
const show = ref(true)
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-if="show" v-model="value" />
</template>
