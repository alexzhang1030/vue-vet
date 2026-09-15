<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
const bind = (el: Element | null) => {
  void el
}
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[revision]">
    <span v-if="visible" :ref="bind">ready</span>
  </div>
</template>
