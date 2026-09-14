<script setup lang="ts">
import { reactive } from 'vue'
import { structuredClone } from './clone'
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
