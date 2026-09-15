<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span ref="node">always</span>
  <span v-if="visible" ref="node">ready</span>
</template>
