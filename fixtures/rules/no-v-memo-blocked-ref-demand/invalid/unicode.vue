<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const 版本 = ref(0)
const 可见 = ref(false)
const 节点 = ref(null)
onMounted(async () => {
  可见.value = true
  await nextTick()
  节点.value.textContent
})
</script>
<template>
  <div v-memo="[版本]">
    <span v-if="可见" ref="节点">ready</span>
  </div>
</template>
