<script setup lang="ts">
import { computed, reactive } from 'vue'
const 表 = reactive(new Map([['selected', 1], ['other', 2]]))
const 选中 = computed(() => {
  let value
  表.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  return value
})
void 选中
</script>

<template>
  <p />
</template>
