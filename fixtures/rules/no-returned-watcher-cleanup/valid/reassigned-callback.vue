<script setup lang="ts">
import { watchEffect } from 'vue'
function cb() {
  return () => {}
}
cb = () => {}
watchEffect(cb)
</script>
<template>ok</template>
