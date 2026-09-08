<script setup lang="ts">
import { reactive, toRaw } from 'vue'
structuredClone(toRaw(reactive({ count: 1 })))
</script>

<template>
  <p />
</template>
