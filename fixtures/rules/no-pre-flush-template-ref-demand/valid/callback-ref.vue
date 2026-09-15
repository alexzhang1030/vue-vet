<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
const bind = (el: Element | null) => {
  void el
}
watch(visible, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" :ref="bind">ready</span>
</template>
