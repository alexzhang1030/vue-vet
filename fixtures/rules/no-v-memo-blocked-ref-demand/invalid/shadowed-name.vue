<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
const trim = (node: string) => node.trim()
const bump = (revision: number) => revision + 1
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[revision]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
