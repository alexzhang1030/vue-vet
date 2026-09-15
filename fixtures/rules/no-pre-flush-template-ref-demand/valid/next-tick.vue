<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, async () => {
  await nextTick()
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
