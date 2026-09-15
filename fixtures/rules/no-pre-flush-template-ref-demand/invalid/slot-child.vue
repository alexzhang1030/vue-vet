<script setup lang="ts">
import { h, onMounted, ref, watch } from 'vue'
const Child = {
  setup(_: unknown, { slots }: { slots: Record<string, () => unknown> }) {
    return () => h('div', slots.default?.())
  },
}
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
  <Child>
    <span v-if="visible" ref="node">ready</span>
  </Child>
</template>
