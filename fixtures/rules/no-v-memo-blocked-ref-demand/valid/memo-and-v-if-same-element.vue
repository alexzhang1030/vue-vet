<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <span v-if="visible" v-memo="[revision]" ref="node">ready</span>
</template>
