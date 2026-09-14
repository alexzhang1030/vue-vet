<script setup lang="ts">
import { shallowRef, triggerRef } from 'vue'
const r = shallowRef({ n: 1 })
triggerRef(r)
</script>

<template>
  <p />
</template>
