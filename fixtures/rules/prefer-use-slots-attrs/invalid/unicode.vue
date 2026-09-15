<script setup lang="ts">
import { getCurrentInstance } from 'vue'
const instance = getCurrentInstance()
console.log(instance?.slots.default)
</script>
<template>
  <p>中文前缀</p>
  <slot />
</template>
