<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.focus()
}, { flush: 'sync' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <button v-if="visible" ref="node">go</button>
</template>
