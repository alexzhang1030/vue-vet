<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const 可见 = ref(false)
const 节点 = ref(null)
watch(可见, () => {
  节点.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  可见.value = true
})
</script>
<template>
  <span v-if="可见" ref="节点">ready</span>
</template>
