<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const count = ref(1)
const node = ref(null)
watch(count, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  count.value = 2
})
</script>
<template>
  <div ref="node">{{ count }}</div>
</template>
