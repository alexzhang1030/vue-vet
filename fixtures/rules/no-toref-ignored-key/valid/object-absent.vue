<script setup lang="ts">
import { toRef } from 'vue'
const source = { n: 1 }
const later = toRef(source, 'later')
void later
</script>

<template>
  <p />
</template>
