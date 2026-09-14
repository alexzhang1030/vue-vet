<script setup lang="ts">
import { reactive } from 'vue'
function triggerRef(_value: unknown) {}
const obj = reactive({ n: 1 })
triggerRef(obj)
</script>

<template>
  <p />
</template>
