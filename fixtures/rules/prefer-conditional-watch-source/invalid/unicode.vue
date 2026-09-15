<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const 开关 = ref(false)
const 源 = ref(2)
const 槽 = ref(5)
const 重 = computed(() => 源.value)
watch([开关, 重], ([激活, 值]) => {
  if (激活) 槽.value = 值
})
源.value = 3
</script>

<template>
  <p />
</template>
