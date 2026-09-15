<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <Child ref="left" />
  <Child ref="right" />
</template>
