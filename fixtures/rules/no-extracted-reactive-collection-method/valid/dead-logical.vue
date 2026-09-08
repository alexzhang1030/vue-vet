<script setup lang="ts">
import { reactive } from 'vue'
const items = reactive(new Map())
const { get } = items
false && get('key')
</script>

<template>
  <p />
</template>
