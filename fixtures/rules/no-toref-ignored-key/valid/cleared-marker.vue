<script setup lang="ts">
import { ref, toRef } from 'vue'
const state = ref({ count: 0 })
state.__v_isRef = false
const count = toRef(state, 'count')
void count
</script>

<template>
  <p />
</template>
