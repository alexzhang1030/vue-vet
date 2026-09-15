<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const 运行 = useDebounceFn((value: string) => value.toUpperCase(), 50)
const 先前 = 运行('aa')
运行('bb')
const 结果 = (await 先前).slice(0, 1)
void 结果
</script>

<template>
  <p />
</template>
