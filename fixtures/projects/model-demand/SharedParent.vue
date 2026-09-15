<script setup lang="ts">
import { ref, onMounted } from 'vue'
import SharedChild from './SharedChild.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <SharedChild ref="left" />
  <SharedChild ref="right" />
</template>
