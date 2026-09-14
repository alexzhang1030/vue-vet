<script setup lang="ts">
import { markRaw, reactive } from 'vue'
structuredClone(reactive(markRaw({ count: 1 })))
</script>

<template>
  <p />
</template>
