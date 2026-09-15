<script setup lang="ts">
import { ref, onMounted } from 'vue'
import FreshChild from './FreshChild.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <FreshChild ref="left" />
  <FreshChild ref="right" />
</template>
