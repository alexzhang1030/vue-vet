<script setup lang="ts">
import { effectScope } from 'vue'
const 作用域 = effectScope()
作用域.stop()
const 结果 = 作用域.run(() => ({ count: 1 }))
void 结果.count
</script>

<template>
  <p />
</template>
