<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { immediate: true })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
