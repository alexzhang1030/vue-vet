<script setup lang="ts">
import { getCurrentInstance } from 'vue'
const instance = getCurrentInstance()
console.log(instance?.slots.default)
</script>
<template>
  <slot />
</template>
