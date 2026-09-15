<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const 修订 = ref(0)
const 源 = ref(1)
const 值 = computedWithControl(修订, () => 源.value)
void 值.value
源.value = 'text'
值.value.toUpperCase()
</script>

<template>
  <p />
</template>
