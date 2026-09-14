<script setup lang="ts">
import { reactive, triggerRef } from 'vue'
declare const externalRef: { value: number }
triggerRef(reactive(externalRef))
</script>

<template>
  <p />
</template>
