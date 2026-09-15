<script setup lang="ts">
import { reactive } from 'vue'
class 计数器 {
  #数量 = 1
  读取() {
    return this.#数量
  }
}
const 代理 = reactive(new 计数器())
void 代理.读取()
</script>

<template>
  <p />
</template>
