<script setup lang="ts">
import { reactive } from 'vue'
const state = reactive({ count: 1 })
let alias = state
;[alias] = [{}]
structuredClone(alias)
</script>

<template>
  <p />
</template>
