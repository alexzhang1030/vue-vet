<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const other = ref(0)
const node = ref(null)
watch(other, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
