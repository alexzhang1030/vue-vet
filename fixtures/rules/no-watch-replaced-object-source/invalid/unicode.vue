<script setup lang="ts">
import { reactive, watch } from 'vue'
const 状态 = reactive({ nested: { x: 1 } })
watch(状态.nested, () => {})
状态.nested = { x: 9 }
</script>

<template>
  <p />
</template>
