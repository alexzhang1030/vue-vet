<script setup lang="ts">
import { reactive, triggerRef } from 'vue'
const 状态 = reactive({ n: 1 })
triggerRef(状态)
</script>

<template>
  <p />
</template>
