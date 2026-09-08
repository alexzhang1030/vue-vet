<script setup lang="ts">
import { reactive, toRaw } from 'vue'
let state: object = reactive({ count: 1 })
state = toRaw(state)
structuredClone(state)
</script>

<template>
  <p />
</template>
