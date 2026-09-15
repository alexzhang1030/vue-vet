<script setup lang="ts">
import { ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { flush: 'pre' })
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
