<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const outer = ref(0)
const inner = ref(0)
const visible = ref(false)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[outer]">
    <div v-memo="[inner]">
      <span v-if="visible" ref="node">ready</span>
    </div>
  </div>
</template>
