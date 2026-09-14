<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function 接收(_value: unknown) {}
watch(
  n,
  (新, 旧) => {
    if (旧 === undefined) return
    接收(新)
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
