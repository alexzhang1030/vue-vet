<script setup lang="ts">
import { type triggerRef as Trigger } from 'vue'
type T = Trigger
const triggerRef = (_value: unknown) => {}
triggerRef({ n: 1 })
</script>

<template>
  <p />
</template>
