<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ n: 1 })
function 接收(_value: unknown) {}
watch(state, (新, 旧) => {
  if (新 === 旧) return
  接收(新)
})
</script>

<template>
  <p />
</template>
