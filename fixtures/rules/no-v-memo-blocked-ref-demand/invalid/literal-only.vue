<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const visible = ref(false)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[1]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
