<script setup lang="ts">
import { useSlots } from 'vue'
const slots = useSlots()
console.log(slots.default)
</script>
<template>
  <slot />
</template>
