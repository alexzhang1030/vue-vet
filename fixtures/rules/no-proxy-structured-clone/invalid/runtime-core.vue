<script setup lang="ts">
import { reactive } from '@vue/runtime-core'
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
