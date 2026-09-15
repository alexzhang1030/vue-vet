<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const 源 = ref(1)
const 显示 = ref(0)
syncRef(源, 显示)
源.value = 5
void 显示.value
</script>

<template>
  <p />
</template>
