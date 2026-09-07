<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const state = reactive({ count: 1 })
void toRefs(state).missing.value
const bag = toRefs(state)
void bag.missing.value
</script>

<template>
  <p />
</template>
