<script setup lang="ts">
import { ref, onMounted } from 'vue'
import LiteralChild from './LiteralChild.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <LiteralChild ref="left" />
  <LiteralChild ref="right" />
</template>
