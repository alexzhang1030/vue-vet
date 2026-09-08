<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const state = reactive({ count: 1 })
const box = { state }
void box
void toRefs(state).missing.value
</script>

<template>
  <p />
</template>
