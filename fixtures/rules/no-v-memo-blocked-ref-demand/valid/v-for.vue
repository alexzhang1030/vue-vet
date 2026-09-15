<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
const items = ref([1])
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[revision]">
    <span v-for="item in items" v-if="visible" ref="node">{{ item }}</span>
  </div>
</template>
