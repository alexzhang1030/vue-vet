<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
const trim = (node: string) => node.trim()
const isOn = (visible: boolean) => visible
watch(visible, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
