<script setup lang="ts">
import { effectScope } from 'vue'
declare const maybe: boolean | (() => void)
effectScope(maybe)
</script>

<template>
  <p />
</template>
