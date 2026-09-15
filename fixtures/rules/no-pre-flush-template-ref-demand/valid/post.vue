<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { flush: 'post' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
