<script setup lang="ts">
import { reactive, triggerRef } from 'vue'
const obj = reactive({ n: 1 })
triggerRef(obj)
</script>

<template>
  <p />
</template>
