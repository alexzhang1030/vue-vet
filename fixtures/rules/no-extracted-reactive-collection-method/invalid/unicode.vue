<script setup lang="ts">
import { reactive } from 'vue'
const 表 = reactive(new Map([['键', 1]]))
const { get } = 表
get('键')
</script>

<template>
  <p />
</template>
