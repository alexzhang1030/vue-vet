<script setup lang="ts">
import { ref, onMounted } from 'vue'
import MyChild from './Child.vue'
const value = ref()
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <my-child v-model="value" />
</template>
