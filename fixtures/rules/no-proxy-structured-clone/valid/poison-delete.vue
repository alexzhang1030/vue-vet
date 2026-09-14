<script setup lang="ts">
import { reactive } from 'vue'
delete globalThis.structuredClone
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
